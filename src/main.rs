macro_rules! info {
    ($($arg:tt)*) => { println!("[info] {}", format_args!($($arg)*)) };
}

macro_rules! warn {
    ($($arg:tt)*) => { println!("[warn] {}", format_args!($($arg)*)) };
}

macro_rules! error {
    ($($arg:tt)*) => { println!("[error] {}", format_args!($($arg)*)) };
}

mod config;
mod handbrake;
mod watch;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::{fs, io, thread};

use config::Config;
use handbrake::{Encode, HandBrake};
use watch::{Candidate, History, Tracker};

const TEMP_DIR: &str = ".muxly-tmp";
const SCAN_INTERVAL: Duration = Duration::from_secs(10);

static STOP: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
}

extern "C" fn request_stop(_: i32) {
    STOP.store(true, Ordering::SeqCst);
}

pub fn stop_requested() -> bool {
    STOP.load(Ordering::SeqCst)
}

enum Outcome {
    Done,
    Failed(String),
    Changed,
    Taken,
    Interrupted,
}

fn main() -> ExitCode {
    unsafe {
        signal(2, request_stop);
        signal(15, request_stop);
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let config = Config::load()?;
    config.check_paths()?;
    let handbrake = HandBrake {
        presets_file: config.config_dir.join("presets.json"),
        log_file: config.config_dir.join("logs").join("handbrake.log"),
    };

    let presets = handbrake.presets()?;
    let known = presets.iter().any(|p| {
        *p == config.preset
            || p.rsplit_once('/')
                .is_some_and(|(_, name)| name == config.preset)
    });
    if !presets.is_empty() && !known {
        return Err(format!(
            "preset '{}' doesn't exist (list them with HandBrakeCLI --preset-list)",
            config.preset
        ));
    }
    info!(
        "'{}' -> '{}' with '{}' as {}, source: {}",
        config.watch_dir.display(),
        config.output_dir.display(),
        config.preset,
        config.extension,
        if config.delete_source {
            "delete"
        } else {
            "keep"
        }
    );
    let _ = fs::remove_dir_all(config.output_dir.join(TEMP_DIR));

    let history_file = config.config_dir.join("history.tsv");
    let mut history = History::load(history_file.clone())
        .map_err(|e| format!("cannot read '{}': {e}", history_file.display()))?;
    let mut tracker = Tracker::default();

    while !stop_requested() {
        let busy = poll(&config, &handbrake, &mut history, &mut tracker);
        let idle_until = Instant::now() + SCAN_INTERVAL;
        while !busy && !stop_requested() && Instant::now() < idle_until {
            thread::sleep(Duration::from_millis(250));
        }
    }
    info!("stopped");
    Ok(())
}

fn poll(
    config: &Config,
    handbrake: &HandBrake,
    history: &mut History,
    tracker: &mut Tracker,
) -> bool {
    let candidates = watch::candidates(&config.watch_dir);
    tracker.keep_only(&candidates);

    for candidate in &candidates {
        let fingerprint = candidate.fingerprint();
        if history.contains(fingerprint) || !tracker.is_stable(candidate) {
            continue;
        }
        let relative = candidate
            .path
            .strip_prefix(&config.watch_dir)
            .unwrap_or(&candidate.path);
        let target = config.output_dir.join(output_name(
            relative,
            candidate.path.is_dir(),
            config.extension,
        ));
        if target.exists() {
            if tracker.block(&candidate.path) {
                warn!(
                    "'{}' is waiting: '{}' already exists",
                    relative.display(),
                    target.display()
                );
            }
            continue;
        }
        tracker.forget(&candidate.path);
        convert(
            config,
            handbrake,
            history,
            candidate,
            relative,
            &target,
            fingerprint,
        );
        return true;
    }
    false
}

fn convert(
    config: &Config,
    handbrake: &HandBrake,
    history: &mut History,
    candidate: &Candidate,
    relative: &Path,
    target: &Path,
    fingerprint: u64,
) {
    let name = relative.display();
    info!("converting '{name}'");
    let started = Instant::now();

    let done = match encode(config, handbrake, candidate, relative, target, fingerprint) {
        Outcome::Done => {
            info!("'{name}' converted in {}", hms(started.elapsed()));
            handle_source(config, &candidate.path, relative);
            true
        }
        Outcome::Failed(reason) => {
            error!(
                "'{name}' failed: {reason}. Details in '{}'; remove its line from '{}' to retry",
                handbrake.log_file.display(),
                history.path().display()
            );
            false
        }
        Outcome::Changed => {
            warn!("'{name}' changed during the conversion, it will be converted again");
            return;
        }
        Outcome::Taken => {
            warn!(
                "'{}' appeared during the conversion of '{name}', the result was discarded",
                target.display()
            );
            return;
        }
        Outcome::Interrupted => return,
    };
    if let Err(e) = history.record(fingerprint, done, &candidate.path) {
        error!("cannot write '{}': {e}", history.path().display());
    }
}

fn encode(
    config: &Config,
    handbrake: &HandBrake,
    candidate: &Candidate,
    relative: &Path,
    target: &Path,
    fingerprint: u64,
) -> Outcome {
    let temp_dir = config.output_dir.join(TEMP_DIR);
    let temp = temp_dir.join(format!("{fingerprint:016x}.{}", config.extension));
    if let Err(e) = fs::create_dir_all(&temp_dir) {
        return Outcome::Failed(format!("cannot create '{}': {e}", temp_dir.display()));
    }

    let mut step = 0;
    let result = handbrake.encode(&candidate.path, &temp, config, |fraction, fps, eta| {
        let current = (fraction * 10.0) as u32;
        if current > step && current < 10 {
            step = current;
            let speed = if fps > 0.0 {
                format!(" ({fps:.1} fps, ETA {})", hms(Duration::from_secs(eta)))
            } else {
                String::new()
            };
            info!("'{}' {}%{speed}", relative.display(), current * 10);
        }
    });
    let outcome = match result {
        Encode::Done
            if watch::snapshot(&candidate.path).ok()
                != Some((candidate.size, candidate.modified)) =>
        {
            Outcome::Changed
        }
        Encode::Done if target.exists() => Outcome::Taken,
        Encode::Done => match move_file(&temp, target) {
            Ok(()) => Outcome::Done,
            Err(e) => Outcome::Failed(format!("cannot write '{}': {e}", target.display())),
        },
        Encode::Failed(reason) => Outcome::Failed(reason),
        Encode::Interrupted => Outcome::Interrupted,
    };
    let _ = fs::remove_file(&temp);
    let _ = fs::remove_dir(&temp_dir);
    outcome
}

fn handle_source(config: &Config, source: &Path, relative: &Path) {
    if !config.delete_source {
        return;
    }
    let removed = if source.is_dir() {
        fs::remove_dir_all(source)
    } else {
        fs::remove_file(source)
    };
    match removed {
        Ok(()) => {
            info!("deleted the source '{}'", relative.display());
            for dir in source
                .ancestors()
                .skip(1)
                .take_while(|d| *d != config.watch_dir && d.starts_with(&config.watch_dir))
            {
                if fs::remove_dir(dir).is_err() {
                    break;
                }
            }
        }
        Err(e) => error!("cannot delete the source '{}': {e}", relative.display()),
    }
}

fn output_name(relative: &Path, is_folder: bool, extension: &str) -> PathBuf {
    let mut name = if is_folder {
        relative.as_os_str().to_owned()
    } else {
        relative.with_extension("").into_os_string()
    };
    name.push(".");
    name.push(extension);
    name.into()
}

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

fn hms(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}
