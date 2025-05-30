use std::{
    fs::{self, File},
    io::{IsTerminal, Stderr, Stdout, Write},
    path::PathBuf,
    process::exit,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::anyhow;
use chrono::Local;
use raylib::ffi::TraceLogLevel;
use tinyfiledialogs::MessageBoxIcon;

use crate::FoximgInstance;

fn use_color() -> AtomicBool {
    let out = LOG_OUT.try_lock().unwrap();
    let c = if let FoximgLogOut::Stdout(ref stdout) = *out {
        stdout.is_terminal()
    } else {
        true
    };

    AtomicBool::new(c)
}

pub enum FoximgLogOut {
    Stdout(Stdout),
    Stderr(Stderr),
}

impl Write for FoximgLogOut {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            FoximgLogOut::Stdout(stdout) => stdout.write(buf),
            FoximgLogOut::Stderr(stderr) => stderr.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            FoximgLogOut::Stdout(stdout) => stdout.flush(),
            FoximgLogOut::Stderr(stderr) => stderr.flush(),
        }
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        match self {
            FoximgLogOut::Stdout(stdout) => stdout.write_vectored(bufs),
            FoximgLogOut::Stderr(stderr) => stderr.write_vectored(bufs),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            FoximgLogOut::Stdout(stdout) => stdout.write_all(buf),
            FoximgLogOut::Stderr(stderr) => stderr.write_all(buf),
        }
    }

    fn write_fmt(&mut self, fmt: std::fmt::Arguments<'_>) -> std::io::Result<()> {
        match self {
            FoximgLogOut::Stdout(stdout) => stdout.write_fmt(fmt),
            FoximgLogOut::Stderr(stderr) => stderr.write_fmt(fmt),
        }
    }
}

static LOG: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::with_capacity(8 * 1024)));
static LOG_COLOR: Mutex<LazyLock<AtomicBool>> = Mutex::new(LazyLock::new(self::use_color));
static LOG_OUT: LazyLock<Mutex<FoximgLogOut>> =
    LazyLock::new(|| Mutex::new(FoximgLogOut::Stderr(std::io::stderr())));

static LOG_QUIET: AtomicBool = AtomicBool::new(false);

pub fn out(out: FoximgLogOut) {
    *LOG_OUT.lock().unwrap() = out;
    *LOG_COLOR.lock().unwrap() = LazyLock::new(self::use_color);
}

pub fn quiet(val: bool) {
    LOG_QUIET.store(val, Ordering::SeqCst);
}

fn show_msg(msg: &str) {
    // tinyfiledialogs doesn't allow any quotes in messages for security reasons:
    // https://github.com/jdm/tinyfiledialogs-rs/issues/19#issuecomment-703524215
    // https://nvd.nist.gov/vuln/detail/cve-2020-36767
    let mut msg = msg.replace('"', "“");
    msg = msg.replace('\'', "＇");

    // tinyfiledialogs-rs 3.9.1 allows shell metacharacters in its message boxes. This allows for
    // OS Command Injection exploits:
    // https://nvd.nist.gov/vuln/detail/CVE-2023-47104
    // https://avd.aquasec.com/nvd/2023/cve-2023-47104/
    if cfg!(not(any(target_os = "windows", target_os = "macos"))) {
        msg = msg.replace('`', "＇");
        msg = msg.replace('$', "＄");
        msg = msg.replace('&', "＆");
        msg = msg.replace(';', ";");
        msg = msg.replace('|', "｜");
        msg = msg.replace('<', "＜");
        msg = msg.replace('>', "＞");
        msg = msg.replace('(', "（");
        msg = msg.replace(')', "）");
    }

    tinyfiledialogs::message_box_ok("foximg - Error", &msg, MessageBoxIcon::Error);
}

#[inline(always)]
fn foximg_logfile_folder_current_ext(name: &str) -> anyhow::Result<PathBuf> {
    let mut folder = std::env::current_exe()?;
    folder.pop();

    Ok(folder.join(name))
}

fn foximg_logfile_folder() -> anyhow::Result<PathBuf> {
    if !cfg!(target_os = "windows") && !cfg!(debug_assertions) {
        let mut path: PathBuf = match std::env::var("XDG_STATE_HOME") {
            Ok(path) => path.into(),
            Err(_) => {
                let Result::<PathBuf, _>::Ok(mut path) =
                    std::env::var("HOME").map(|path| path.into())
                else {
                    tracelog(
                        TraceLogLevel::LOG_WARNING,
                        "FOXIMG: \"HOME\" enviroment variable not set. Using log folder in executable's directory",
                    );
                    return foximg_logfile_folder_current_ext(".foximg_logs");
                };

                path.push(".local/state");
                path
            }
        };
        path.push("foximg/logs");
        Ok(path)
    } else {
        foximg_logfile_folder_current_ext("logs")
    }
}

fn foximg_logfile(
    crash: bool,
    time: chrono::DateTime<Local>,
    msg: &str,
) -> anyhow::Result<PathBuf> {
    let folder = self::foximg_logfile_folder()?;
    let log_type = if crash { "CRASH" } else { "LOG" };
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

fn foximg_logfile_msg(log: anyhow::Result<PathBuf>) -> String {
    match log {
        Ok(path) => format!("Created log file in {:?}", path),
        Err(e) => format!("Couldn't create log file: {:?}", e),
    }
}

#[cold]
#[inline(never)]
pub fn panic(panic_info: &std::panic::PanicHookInfo) {
    let time = Local::now();
    let time_str = format!("{}: ", time.format("%H:%M:%S"));
    let panic_str = panic_info.to_string();

    let _ = self::print_log(&time_str, TraceLogLevel::LOG_ERROR, "PANIC: ", &panic_str);
    let log = self::foximg_logfile(true, time, &format!("{time_str}PANIC: {panic_str}"));
    self::show_msg(&format!("{panic_str}\n\n{}", self::foximg_logfile_msg(log)));
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
    self::print_log(&time_str, level, level_str, msg).unwrap();

    if level == LOG_ERROR {
        self::show_msg(msg);
    } else if level == LOG_FATAL {
        // Fatal logs exit the process without running destructors. Therefore we want to delete the
        // instance folder ourselves, since it won't get deleted by the FoximgInstance destructor.
        if let Err(e) = self::fatal_delete_instance_folder() {
            tracelog(
                LOG_WARNING,
                "FOXIMG: Failed to delete instance marker folder:",
            );
            tracelog(LOG_WARNING, &format!("    > {e}"));
        }

        let log = self::foximg_logfile(true, time, &msg_fmt);
        self::show_msg(&format!("{msg}\n\n{}", self::foximg_logfile_msg(log)));
        exit(1);
    }

    self::LOG.lock().unwrap().push_str(&msg_fmt);
}

fn print_log(
    time_str: &str,
    level_color: TraceLogLevel,
    level_str: &str,
    msg: &str,
) -> anyhow::Result<()> {
    use TraceLogLevel::*;

    if !(cfg!(all(debug_assertions, target_os = "windows")) || cfg!(not(target_os = "windows")))
        || self::LOG_QUIET.load(Ordering::SeqCst)
    {
        return Ok(());
    }

    let color = LOG_COLOR
        .lock()
        .map_err(|e| anyhow!("{e}"))?
        .load(Ordering::SeqCst);

    let mut out = LOG_OUT.lock().map_err(|e| anyhow!("{e}"))?;
    if color {
        const TIME_COLOR: &str = "\x1b[3m\x1b[38;5;52m";
        const RESET_COLOR: &str = "\x1b[0m";

        let level_color = match level_color {
            LOG_TRACE => "\x1b[3m\x1b[38;5;8m",
            LOG_DEBUG => "\x1b[38;5;20m",
            LOG_INFO => "\x1b[38;5;114m",
            LOG_WARNING => "\x1b[38;5;202m",
            LOG_ERROR | LOG_FATAL => "\x1b[1m\x1b[38;5;202m",
            _ => "",
        };

        writeln!(
            out,
            "{TIME_COLOR}{time_str}{RESET_COLOR}{level_color}{level_str}{RESET_COLOR}{msg}"
        )?;
    } else {
        writeln!(out, "{time_str}{level_str}{msg}")?;
    }
    Ok(())
}

fn fatal_delete_instance_folder() -> std::io::Result<()> {
    let instance_folder = FoximgInstance::instances_path()?;
    std::fs::remove_dir_all(instance_folder)?;
    Ok(())
}
