use std::{
    process::Command
    , process::Output
    , time::Duration
    , thread::sleep
    , fs::File
    , collections::HashMap
    , time::Instant
};
use log::{info, error, LevelFilter};
use log4rs::append::{console::ConsoleAppender, file::FileAppender};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::config::{Appender, Config, Logger, Root};
use serde_yaml;
use serde::Deserialize;

#[derive (Deserialize, Debug)]
struct loop_config {
    source_code_path: String
    , release_bin_storage_path: Option<String>
}


fn construct_command_and_get_output(command_string: &str, project_location: &str, project_name: &str) -> Output{
    let target_string = format!("ci-cd_worker::{project_name}");
    let target_str = target_string.as_str();
    if command_string == "" {
        error!(target: target_str, "Command given is an empty string. What am I supposed to do with this, bro?");
        panic!()
    }
    let mut command_string_split = command_string.split(' ');
    // the below uses an unwrap because if the string is not empty, which is checked above,
    // then the returned value of the next() operation is at minimum Some("")
    let mut command_construct = Command::new(command_string_split.next().unwrap());
    for elem in command_string_split {
        command_construct.arg(elem);
    };
    command_construct.current_dir(project_location);
    let command_output = command_construct.output();
    match command_output {
        Ok(output) => return output
        , _ => {
            error!(target: target_str, "Error when running command created from command string {command_string}.");
            panic!()
        }
    }
}

fn get_command_stdout(command_output: Output, acceptable_status_codes: Vec<i32>, command_name: &str, project_name: &str) -> String {
    let target_string = format!("ci-cd_worker::{project_name}");
    let target_str = target_string.as_str();
    let output_message = match String::from_utf8(command_output.stdout) {
        Ok(output_string) => output_string
        , _ => {
            error!(target: target_str, "Could not convert the stdout to string for command {command_name}.");
            panic!()
        }
    };
    let output_error = match String::from_utf8(command_output.stderr) {
        Ok(output_string) => output_string
        , _ => {
            error!(target: target_str, "Could not convert the stderr to string for command {command_name}.");
            panic!()
        }
    };
    let output_status_code = match command_output.status.code() {
        Some(code) => code
        , _ => {
            error!(target: target_str, "Status code returned null for command {command_name}.");
            panic!()
        }
    };
    if acceptable_status_codes.contains(&output_status_code) {
        output_message
    } else {
        let mut error_string: String = format!("The command {command_name} did not finish successfully.");
        if output_message.len() > 0 {
            error_string = error_string + &format!("\nstdout was:\n{output_message}")
        };
        if output_error.len() > 0 {
            error_string = error_string + &format!("\nstderr was:\n{output_error}")
        };
        error!(target: target_str, "{error_string}");
        panic!()
    }
}

fn main() {
    let config_file = File::open("/etc/ci-cd_worker/list_of_projects_to_update.yaml")
        .expect("Could not open the file with the run config");
    let run_config: HashMap<String, loop_config> = serde_yaml::from_reader(config_file)
        .expect("Could not deserialize config file content into loop config struct");
    let stdout = ConsoleAppender::builder().build();
    let mut incremental_logger_config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)));
    for (project_name, &ref config) in run_config.iter() {
        let logger_name = format!("ci-cd_worker::{project_name}");
        let logger_name_str = logger_name.as_str();
        let file_appender = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {l} {t} - {m}{n}")))
            .build(format!("/var/log/cc_app_logs/ci-cd_worker/{project_name}.log"))
            .expect("Could not create the file appender for the {project_name} logger");
        incremental_logger_config = incremental_logger_config
            .appender(Appender::builder().build(project_name, Box::new(file_appender)))
            .logger(Logger::builder()
                .appender("stdout")
                .appender(project_name)
                .additive(false)
                .build(logger_name_str, LevelFilter::Info)
            )
    };
    let logger_config = incremental_logger_config.build(Root::builder().appender("stdout").build(LevelFilter::Info))
        .expect("Could not create the logger config");

    // let handle = log4rs::init_config(logger_config).expect("Error when initializing the logger");
    log4rs::init_config(logger_config).expect("Error when initializing the logger");
    for (project_name, & ref config) in run_config.iter() {
        let logger_name = format!("ci-cd_worker::{project_name}");
        let logger_name_str = logger_name.as_str();
        info!(target: logger_name_str, "Beginning update for project {project_name}.");
        let _ = construct_command_and_get_output("git fetch --all", &config.source_code_path, project_name);
        let git_status = construct_command_and_get_output("git status", &config.source_code_path, project_name);
        let git_status_output = get_command_stdout(git_status, vec![0], "git status", project_name);
        if !git_status_output.contains("Your branch is up to date") {
            let _ = construct_command_and_get_output("git pull", &config.source_code_path, project_name);
            info!(target: logger_name_str, "Pulled latest changes. Attempting to stop cron.");
            match &config.release_bin_storage_path {
                Some(release_bin_storage_path) => {
                    let _ = construct_command_and_get_output("sudo systemctl stop cron.service", &config.source_code_path, project_name);
                    let cron_status_command = construct_command_and_get_output("sudo systemctl status cron.service", &config.source_code_path, project_name);
                    let mut cron_status_output = get_command_stdout(cron_status_command, vec![0, 3], "cron status after shutting down", project_name);
                    let loop_start_time = Instant::now();
                    info!(target: logger_name_str, "Checking if cron has stopped.");
                    while !cron_status_output.contains("Active: inactive (dead)")
                    && loop_start_time.elapsed() < Duration::from_secs(5) {
                        sleep(Duration::from_secs_f32(0.5));
                        let cron_status_command = construct_command_and_get_output("sudo systemctl status cron.service", &config.source_code_path, project_name);
                        cron_status_output = get_command_stdout(cron_status_command, vec![0, 3], "cron status after shutting down", project_name);
                    }
                    if !cron_status_output.contains("Active: inactive (dead)") {
                        error!(target: logger_name_str, "Could not stop service for project {project_name}.");
                        panic!()
                    }
                    info!(target: logger_name_str, "Cron has stopped. Attempting build.");
                    let _ = construct_command_and_get_output("cargo build --release", &config.source_code_path, project_name);
                    info!(target: logger_name_str, "Build done. Attempting to move the binary to it's new home.");
                    let binary_name_split = &config.source_code_path.split('/');
                    let mut count = 0;
                    let mut binary_name = "";
                    for (i,elem) in binary_name_split.clone().enumerate() { count = i; binary_name = elem};
                    count += 1;
                    if count == 1 {
                        error!("The source code path must be an absolute path.");
                        panic!();
                    }
                    let move_command = format!("mv target/release/{binary_name} {release_bin_storage_path}/{project_name}");
                    let move_command_str = move_command.as_str();
                    let _ = construct_command_and_get_output(move_command_str, &config.source_code_path, project_name);
                    info!(target: logger_name_str, "Move finished. Attempting to start cron.");
                    let _ = construct_command_and_get_output("sudo systemctl start cron.service", &config.source_code_path, project_name);
                    let cron_status_command = construct_command_and_get_output("sudo systemctl status cron.service", &config.source_code_path, project_name);
                    let mut cron_status_output = get_command_stdout(cron_status_command, vec![0, 3], "cron status after shutting down", project_name);
                    let loop_start_time = Instant::now();
                    info!(target: logger_name_str, "Checking if cron has started");
                    while !cron_status_output.contains("Active: active (running)")
                    && loop_start_time.elapsed() < Duration::from_secs(5) {
                        sleep(Duration::from_secs_f32(0.5));
                        let cron_status_command = construct_command_and_get_output("sudo systemctl status cron.service", &config.source_code_path, project_name);
                        cron_status_output = get_command_stdout(cron_status_command, vec![0, 3], "cron status after starting up", project_name);
                    }
                    if !cron_status_output.contains("Active: active (running)") {
                        error!(target: logger_name_str, "Could not start cron for project. Failed at project {project_name}.");
                        panic!()
                    }
                    info!(target: logger_name_str, "Cron has started. Project {project_name} updated successfully.");
                }
                , None => info!("No build requested. Project {project_name} updated successfully.")
            };
        } else {
            info!(target: logger_name_str, "Nothing to pull for project {project_name}.")
        }
    }
}
