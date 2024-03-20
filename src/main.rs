use std::process::Command;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Local;
use std::process::Output;

fn log_error_to_file_and_panic(error_message: &str) -> () {
    let error_file_name = "error.txt";
    let error_file_open_error_message = format!("Could not open file {}", error_file_name);
    let mut error_file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(error_file_name)
        .expect(&error_file_open_error_message);
    let current_local_time = Local::now().to_rfc2822();
    let error_string = format!("{}: {}\n", current_local_time, error_message);
    error_file.write_all(&error_string.into_bytes()).expect(format!("Could not write error to file {}", error_file_name).as_str());
    panic!("{}", error_message);
}

fn get_command_message(command_output: Result<Output>, command_name: &str) -> String {
    let command_output = match command_output {
        Ok(output) => output
        , _ => {
            log_error_to_file_and_panic(
                format!(
                    "Error when Rust runs the command {command_name}"
                )
            );
            panic() // Line will never be reached as the above function ends in a panic
        }
    let status_code = match command_output {
        
    }
    let status_message = match git_status {
        Ok(output) => match String::from_utf8(output.stdout) {
            Ok(output_string) => output_string
            , _ => {
                log_error_to_file_and_panic("Converting git status process output to string from bytes");
                panic() // Line will never be reached as the above function ends in a panic
            }
        }
        , _ => {
            log_error_to_file_and_panic("Error when runing git status");
            panic() // Line will never be reached as the above function ends in a panic
        }
    };
}

fn main() {
    let git_status = Command::new("git")
        .arg("status")
        .current_dir("/opt/projects")
        .output();
    
    println!("{status_message:#?}");
    if !status_message.contains("Your branch is up to date") {

    }
    
    // let mut error_file = OpenOptions::new()
    //     .create(true)
    //     .read(true)
    //     .append(true)
    //     .open("file_loc_test.txt")
    //     .unwrap();
    // let _ = error_file.write_all(b"Where are you\n");
}
