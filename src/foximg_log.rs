use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::exit,
    sync::Mutex,
};

use anyhow::anyhow;
use chrono::Local;
use raylib::ffi::TraceLogLevel;

use crate::foximg_error;

static LOG: Mutex<String> = Mutex::new(String::new());

fn foximg_logfile(
    crash: bool,
    time: chrono::DateTime<Local>,
    msg: &str,
) -> anyhow::Result<PathBuf> {
    let folder = Path::new("logs");
    let log_type = match crash {
        true => "CRASH",
        false => "LOG",
    };
    let filename = format!("{log_type} {}.log", time.format("%d.%m.%Y %H.%M.%S"));
    let path = folder.join(filename);
    if !folder.exists() {
        fs::create_dir(folder)?;
    }

    let mut file = File::create(&path)?;
    write!(&mut file, "{}", *LOG.lock().map_err(|e| anyhow!("{e}"))?)?;
    writeln!(&mut file, "{}", msg)?;
    if crash {
        write!(
            &mut file,
            "\n{}",
            std::backtrace::Backtrace::force_capture()
        )?;
    }

    Ok(path)
}

pub const CREATED_LOG_FILE_MSG: &str = "Created log file";

fn foximg_logfile_msg(log: anyhow::Result<PathBuf>) -> String {
    match log {
        Ok(path) => format!("{CREATED_LOG_FILE_MSG} in {:?}", path),
        Err(e) => format!("Couldn't create log file: {:?}", e),
    }
}

pub fn create_file() -> anyhow::Result<()> {
    let time = Local::now();
    let time_str = format!("{}: ", time.format("%H:%M:%S"));
    foximg_logfile(
        false,
        time,
        &format!("{time_str}INFO: {CREATED_LOG_FILE_MSG}"),
    )
    .map(|_| ())
}

#[cold]
#[inline(never)]
pub fn panic(panic_info: &std::panic::PanicInfo) {
    let time = Local::now();
    let time_str = format!("{}: ", time.format("%H:%M:%S"));
    let log = foximg_logfile(true, time, &format!("{time_str}PANIC: {panic_info}"));
    foximg_error::show(&format!("{panic_info}\n\n{}", foximg_logfile_msg(log)));
}

pub fn tracelog(level: TraceLogLevel, msg: &str) {
    use TraceLogLevel::*;

    let time = Local::now();
    let time_str = format!("{}: ", time.format("%H:%M:%S"));
    let level_str = match level {
        LOG_TRACE => "TRACE: ",
        LOG_DEBUG => "DEBUG: ",
        LOG_INFO => "INFO: ",
        LOG_WARNING => "WARNING: ",
        LOG_ERROR => "ERROR: ",
        LOG_FATAL => "FATAL: ",
        _ => "",
    };
    let msg_fmt = format!("{time_str}{level_str}{msg}\n");

    if level == LOG_ERROR {
        foximg_error::show(msg);
    } else if level == LOG_FATAL {
        let log = foximg_logfile(true, time, &msg_fmt);
        foximg_error::show(&format!("{msg}\n\n{}", foximg_logfile_msg(log)));
        exit(1);
    }

    #[cfg(any(debug_assertions, not(target_os = "windows")))]
    print!("{msg_fmt}");
    push(&msg_fmt);
}

pub fn push(string: &str) {
    let mut log = LOG.lock().unwrap();
    log.push_str(string);
}
