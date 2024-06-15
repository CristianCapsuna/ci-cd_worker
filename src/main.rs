#![allow(non_camel_case_types)]
use std::{
    process::Command
    , time::Duration
    , thread::sleep
    , fs::File
    , fs::remove_file
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
) -> Result<String, String> {
    let mut command_string_split = command_string.split(' ');
    let command = match command_string_split.next() {
        Some(string) => string
        , None => {
            let err_message = "Empty command was given".to_string();
            error!(target: logger_name_str, "{err_message}");
            return Err(err_message)
        }
    };
    let mut command = Command::new(command);
    command.args(command_string_split);
    command.current_dir(project_location);
    match command.output() {
        Ok(output) => {
            let output_message = String::from_utf8(output.stdout).expect("Terminal output is always valid utf8");
            let output_error = String::from_utf8(output.stderr).expect("Terminal output is always valid utf8");
            let output_status_code = match output.status.code() {
                Some(code) => code
                , None => {
                    let err_message = format!("Status code returned null for command {command_string}");
                    error!(target: logger_name_str, "{err_message}");
                    return Err(err_message)
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
                return Err(error_string)
            }
        }
        , Err(error) => {
            error!(target: logger_name_str, "Creating output for command {command_string}. \
                Error was:\n{error}");
            return Err(error.to_string())
        }
    }
}

fn get_current_commit_hash(project_location: &str, logger_name: &str) -> Result<String, String> {
    let git_log = command_and_output(
        "git log"
        , project_location
        , vec![0]
        , logger_name)?;
    let commit_line = git_log.split("\n").next().unwrap_or("");
    let commit_hash = commit_line.split(" ").nth(1).unwrap_or("");
    Ok(commit_hash.to_string())
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
    // Updating projects
    for (project_name, & ref config) in run_config.iter() {
        let logger_name = format!("ci-cd_worker::{project_name}");
        let logger_name_str = logger_name.as_str();
        debug!(target: logger_name_str, "Beginning update for project.");
        let fetch = match command_and_output(
            "git fetch --all"
            , &config.source_code_path
            , vec![0]
            , logger_name_str
        ) {
            Ok(string) => string
            , _ => continue
        };
        debug!(target: logger_name_str, "git fetch output:\n{fetch}");
        let status = match command_and_output(
            "git status"
            , &config.source_code_path
            , vec![0]
            , logger_name_str
        ) {
            Ok(string) => string
            , _ => continue
        };
        debug!(target: logger_name_str, "git status output:\n{status}");
        if !status.contains("Your branch is up to date") {
            let current_commit_hash = match get_current_commit_hash(
                &config.source_code_path
                , logger_name_str
            ) {
                Ok(string) => string
                , _ => continue
            };
            let reset_command_string = format!("git reset --hard {current_commit_hash}");
            let reset_command = reset_command_string.as_str();
            let pull = match command_and_output(
                "git pull"
                , &config.source_code_path
                , vec![0]
                , logger_name_str
            ) {
                Ok(string) => string
                , _ => continue
            };
            debug!(target: logger_name_str, "git pull output\n{pull}");
            match &config.release_bin_storage_path {
                Some(release_bin_storage_path) => {
                    let cargo_build = match command_and_output(
                        "cargo build --release"
                        , &config.source_code_path
                        , vec![0]
                        , logger_name_str
                    ) {
                        Ok(string) => string
                        , _ => {
                            let _ = match command_and_output(
                                reset_command
                                , &config.source_code_path
                                , vec![0]
                                , logger_name_str
                            ) {
                                Ok(string) => string
                                , _ => continue
                            };
                            continue
                        }
                    };
                    debug!(target: logger_name_str, "cargo build output:\n{cargo_build}");
                    debug!(target: logger_name_str, "Build done. Attempting to move the binary to it's new home.");
                    let binary_name = match config.source_code_path.split('/').last() {
                        Some(string) => string
                        , _ => {
                            error!(target: logger_name_str, "Binary name cannot be empty");
                            let _ = match command_and_output(
                                reset_command
                                , &config.source_code_path
                                , vec![0]
                                , logger_name_str
                            ) {
                                Ok(string) => string
                                , _ => continue
                            };
                            continue
                        }
                    };
                    let move_command_string = format!(
                        "mv target/release/{binary_name} {release_bin_storage_path}/{project_name}"
                    );
                    let move_command = move_command_string.as_str();
                    let mut lock_acquired = false;
                    let lock_file_name = format!("{release_bin_storage_path}/{project_name}.lock");
                    let loop_start_time = Instant::now();
                    while lock_acquired == false && loop_start_time.elapsed() < Duration::from_secs(5) {
                        let lock_file = File::create_new(&lock_file_name);
                        lock_acquired = match lock_file {
                            Ok(_) => true
                            , Err(_) => {
                                sleep(Duration::from_secs_f32(0.5));
                                false
                            }
                        }
                    }
                    if lock_acquired == false {
                        error!(target: logger_name_str, "Lock file could not be create for project {project_name} since it is already\
                        present and did not disappear within 5 seconds of the program start");
                        continue
                    }
                    let move_op = match command_and_output(
                        move_command
                        , &config.source_code_path
                        , vec![0]
                        , logger_name_str
                    ) {
                        Ok(string) => string
                        , _ => {
                            let _ = match command_and_output(
                                reset_command
                                , &config.source_code_path
                                , vec![0]
                                , logger_name_str
                            ) {
                                Ok(string) => string
                                , _ => continue
                            };
                            continue
                        }
                    };
                    let mut lock_released = false;
                    let loop_start_time = Instant::now();
                    while lock_released == false && loop_start_time.elapsed() < Duration::from_secs(5) {
                        let lock_file_removal = remove_file(&lock_file_name);
                        lock_released = match lock_file_removal {
                            Ok(_) => true
                            , Err(_) => {
                                sleep(Duration::from_secs_f32(0.5));
                                false
                            }
                        }

                    }
                    if lock_released == false {
                        error!(target: logger_name_str, "Lock file could not be released for project\
                        {project_name} within 5 seconds alloted");
                        continue
                    }
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
}
