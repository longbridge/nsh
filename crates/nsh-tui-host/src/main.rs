//! Runs a prebuilt Native Shell bundle in the terminal.

use std::{collections::VecDeque, sync::Mutex};

struct Logs(Mutex<VecDeque<String>>);
static LOGS: Logs = Logs(Mutex::new(VecDeque::new()));
const LOG_CAPACITY: usize = 256;

impl log::Log for Logs {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let mut logs = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if logs.len() == LOG_CAPACITY {
            logs.pop_front();
        }
        logs.push_back(format!(
            "{} {}: {}",
            record.level(),
            record.target(),
            record.args()
        ));
    }
    fn flush(&self) {}
}

fn main() -> std::process::ExitCode {
    let _ = log::set_logger(&LOGS);
    let result = nsh_tui_host::main();
    for message in LOGS.0.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
        eprintln!("{message}");
    }
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nsh-tui-host: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
