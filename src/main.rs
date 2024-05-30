use std::{
    process::Command
    , time::Duration
    , thread::sleep
    , fs::File
    , collections::HashMap
    , time::Instant
};
use log::{debug, info, error, LevelFilter};
use log4rs::append::{console::ConsoleAppender, file::FileAppender};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::config::{Appender, Config, Logger, Root};
use serde_yaml;
use serde::Deserialize;
use std::env;

#[derive (Deserialize, Debug)]
struct loop_config {
    source_code_path: String
    , release_bin_storage_path: Option<String>
}

fn command_and_output(
    command_string: &str
    , project_location: &str
    , acceptable_status_codes: Vec<i32>
    , logger_name_str: &str
) -> Result<String, ()> {
    let mut command_string_split = command_string.split(' ');
    let command = match command_string_split.next() {
        Some(string) => string
        , None => {
            let err_message = "Empty command was given".to_string();
            error!(target: logger_name_str, "{err_message}");
            panic!("{err_message}")
        }
    };
    let mut command = Command::new(command);
    for elem in command_string_split {
        command.arg(elem);
    };
    command.current_dir(project_location);
    debug!("{project_location}");
    match command.output() {
        Ok(output) => {
            let output_message = String::from_utf8(output.stdout).expect("Terminal output is always valid utf8");
            let output_error = String::from_utf8(output.stderr).expect("Terminal output is always valid utf8");
            let output_status_code = match output.status.code() {
                Some(code) => code
                , _ => {
                    error!(target: logger_name_str, "Status code returned null for command {command_string}.");
                    return Err(())
                }
            };
            if acceptable_status_codes.contains(&output_status_code) {
                Ok(output_message)
            } else {
                let mut error_string = format!("The command {command_string} did not finish with expected status code.");
                if output_message.len() > 0 {
                    error_string = error_string + &format!("\nstdout was:\n{output_message}")
                };
                if output_error.len() > 0 {
                    error_string = error_string + &format!("\nstderr was:\n{output_error}")
                };
                error!(target: logger_name_str, "{error_string}");
                return Err(())
            }
        }
        , Err(error) => {
            error!(target: logger_name_str, "Creating output for command {command_string}");
            debug!(target: logger_name_str, "{}", error);
            return Err(())
        }
    }
}

fn check_cron_status(desired_status: &str) -> () {
    let cron_status_result = command_and_output(
        "sudo systemctl status cron.service"
        , "/"
        , vec![0, 3]
        , "root"
    );
    let mut cron_status = match cron_status_result {
        Ok(string) => string
        , _ => {
            let err_message = "Could not check the status of cron".to_string();
            error!("{err_message}");
            panic!("{err_message}")
        }
    };
    debug!("cron status output:\n{cron_status}");
    let loop_start_time = Instant::now();
    while !cron_status.contains(desired_status)
    && loop_start_time.elapsed() < Duration::from_secs(5) {
        sleep(Duration::from_secs_f32(0.5));
        let cron_status_result = command_and_output(
            "sudo systemctl status cron.service"
            , "/"
            , vec![0, 3]
            , "root"
        );
        cron_status = match cron_status_result {
            Ok(string) => string
            , _ => {
                let err_message = "Could not check the status of cron from within the loop".to_string();
                error!("{err_message}");
                panic!("{err_message}")
            }
        };
        debug!("cron status output from withing checking loop:\n{cron_status}");
    }
    if !cron_status.contains(desired_status) {
        let err_message = "Cron not stopped after successful command".to_string();
        error!("{err_message}");
        panic!("{err_message}")
    }
}

fn main() {
    ///////// Parameters

    let logger_config_file = "/etc/ci-cd_worker/list_of_projects_to_update.yaml";
    let logs_root_location = "/var/log/cc_app_logs/ci-cd_worker/";
    let main_log_file_name = "main.log";
    let file_log_output_format = "{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {l} {t} - {m}{n}";
    let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "3".to_string());
    
    ///////// Logger

    let level_filter = match log_level.as_str() {
        "0" => LevelFilter::Off,
        "1" => LevelFilter::Error,
        "2" => LevelFilter::Warn,
        "4" => LevelFilter::Debug,
        "5" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };

    // Opening the logger yaml config file
    let config_file = File::open(logger_config_file)
        .expect("Could not open the file with the run config");
    // Loading the config into a Hashmap
    let run_config: HashMap<String, loop_config> = serde_yaml::from_reader(config_file)
        .expect("Could not deserialize config file content into loop config struct");
    // Creating the main log appenders
    let stdout = ConsoleAppender::builder().build();
    let log_file_appender = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(file_log_output_format)))
        .build(format!("{logs_root_location}{main_log_file_name}"))
        .expect("Could not create the file appender");
    // Starting the logger configuration by adding the console logger
    let mut incremental_logger_config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("main_log", Box::new(log_file_appender)));
    // For every project that this code will manage we want to create an additional logger
    for (project_name, _) in run_config.iter() {
        // Setting the logger name based on the config file
        let logger_name = format!("ci-cd_worker::{project_name}");
        let logger_name_str = logger_name.as_str();
        // Creating the file logger for for the project with a specific output format and location
        let file_appender = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new(file_log_output_format)))
            .build(format!("{logs_root_location}{project_name}.log"))
            .expect(format!("Could not create the file appender for the {project_name} logger").as_str());
        // Creates the appender for our new file and creates a new logger that used is together with the console logger
        incremental_logger_config = incremental_logger_config
            .appender(Appender::builder().build(project_name, Box::new(file_appender)))
            .logger(Logger::builder()
                .appender("stdout")
                .appender(project_name)
                .build(logger_name_str, level_filter)
            )
    };
    let logger_config = incremental_logger_config.build(Root::builder()
        .appender("stdout")
        .appender("main_log")
        .build(level_filter))
        .expect("Could not create the logger config");
    log4rs::init_config(logger_config).expect("Error when initializing the logger");
    
    ///////// Main logic
    // Stopping cron
    let cron_stop_result = command_and_output(
        "sudo systemctl stop cron.service"
        , "/"
        , vec![0]
        , "root"
    );
    let _ = match cron_stop_result {
        Ok(string) => string
        , _ => {
            let err_message = "Could not stop cron".to_string();
            error!("{err_message}");
            panic!("{err_message}")
        }
    };
    debug!("Checking if cron has stopped.");
    check_cron_status("Active: inactive (dead)");
    // Updating projects
    for (project_name, & ref config) in run_config.iter() {
        let logger_name = format!("ci-cd_worker::{project_name}");
        let logger_name_str = logger_name.as_str();
        debug!(target: logger_name_str, "Beginning update for project.");
        let fetch_result = command_and_output(
            "git fetch --all"
            , &config.source_code_path
            , vec![0]
            , logger_name_str
        );
        let fetch = match fetch_result {
            Ok(string) => string
            , _ => continue
        };
        debug!(target: logger_name_str, "git fetch output:\n{fetch}");
        let status_result = command_and_output(
            "git status"
            , &config.source_code_path
            , vec![0]
            , logger_name_str
        );
        let status = match status_result {
            Ok(string) => string
            , _ => continue
        };
        debug!(target: logger_name_str, "git status output:\n{status}");
        if !status.contains("Your branch is up to date") {
            let pull_result = command_and_output(
                "git pull"
                , &config.source_code_path
                , vec![0]
                , logger_name_str
            );
            let pull = match pull_result {
                Ok(string) => string
                , _ => continue
            };
            debug!(target: logger_name_str, "git pull output\n{pull}");
            match &config.release_bin_storage_path {
                Some(release_bin_storage_path) => {
                    let cargo_build_result = command_and_output(
                        "cargo build --release"
                        , &config.source_code_path
                        , vec![0]
                        , logger_name_str
                    );
                    let cargo_build = match cargo_build_result {
                        Ok(string) => string
                        , _ => continue
                    };
                    debug!(target: logger_name_str, "cargo build output:\n{cargo_build}");
                    debug!(target: logger_name_str, "Build done. Attempting to move the binary to it's new home.");
                    let binary_name_option = &config.source_code_path.split('/').last();
                    let binary_name = match binary_name_option {
                        Some(string) => string
                        , _ => {
                            error!(target: logger_name_str, "Binary name cannot be empty");
                            continue
                        }
                    };
                    let move_command = format!("mv target/release/{binary_name} {release_bin_storage_path}/{project_name}");
                    let move_result = command_and_output(
                        move_command.as_str()
                        , &config.source_code_path
                        , vec![0]
                        , logger_name_str
                    );
                    let move_op = match move_result {
                        Ok(string) => string
                        , _ => continue
                    };
                    debug!(target: logger_name_str, "move operation output:\n{move_op}");
                    debug!(target: logger_name_str, "Move finished. Attempting to start cron.");
                }
                , None => debug!(target: logger_name_str, "No build requested. Project updated successfully.")
            };
            info!(target: logger_name_str, "Project was updated.")
        } else {
            info!(target: logger_name_str, "Nothing to pull for project.")
        }
    }
    let cron_start_result = command_and_output(
        "sudo systemctl start cron.service"
        , "/"
        , vec![0]
        , "root"
    );
    let _ = match cron_start_result {
        Ok(string) => string
        , _ => {
            let err_message = "Could not start cron".to_string();
            error!("{err_message}");
            panic!("{err_message}")
        }
    };
    debug!("Checking if cron has started.");
    check_cron_status("Active: active (running)");
}
