use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use crate::config::Config;
use crate::stop_requested;

pub struct HandBrake {
    pub presets_file: PathBuf,
    pub log_file: PathBuf,
}

pub enum Encode {
    Done,
    Failed(String),
    Interrupted,
}

impl HandBrake {
    fn command(&self) -> Command {
        let mut command = Command::new("HandBrakeCLI");
        command.stdin(Stdio::null());
        if self.presets_file.is_file() {
            command.arg("--preset-import-file").arg(&self.presets_file);
        }
        command
    }

    pub fn presets(&self) -> Result<Vec<String>, String> {
        let output = self
            .command()
            .arg("--preset-list")
            .output()
            .map_err(|e| format!("cannot run HandBrakeCLI: {e}"))?;
        let text =
            String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr);
        Ok(parse_preset_list(&text))
    }

    pub fn encode(
        &self,
        input: &Path,
        output: &Path,
        config: &Config,
        mut progress: impl FnMut(f64, f64, u64),
    ) -> Encode {
        let log = match self.open_log(input) {
            Ok(file) => file,
            Err(e) => {
                return Encode::Failed(format!("cannot open '{}': {e}", self.log_file.display()));
            }
        };
        let mut command = self.command();
        command
            .arg("--json")
            .arg("-i")
            .arg(input)
            .arg("-o")
            .arg(output)
            .arg("--main-feature")
            .arg("--preset")
            .arg(&config.preset)
            .arg("--format")
            .arg(format!("av_{}", config.extension))
            .stdout(Stdio::piped())
            .stderr(log);

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => return Encode::Failed(format!("cannot run HandBrakeCLI: {e}")),
        };
        let stdout = child.stdout.take().expect("stdout is piped");
        let (sender, receiver) = mpsc::channel::<Value>();
        thread::spawn(move || {
            let mut block: Option<String> = None;
            for line in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
                let line = String::from_utf8_lossy(&line);
                if let Some(text) = block.as_mut() {
                    text.push_str(&line);
                    text.push('\n');
                }
                match line.trim_end() {
                    "Progress: {" => block = Some("{\n".into()),
                    "}" => {
                        if let Some(value) = block
                            .take()
                            .and_then(|text| serde_json::from_str(&text).ok())
                        {
                            let _ = sender.send(value);
                        }
                    }
                    _ => {}
                }
            }
        });

        let mut error_code = None;
        let status = loop {
            if stop_requested() {
                let _ = child.kill();
                let _ = child.wait();
                return Encode::Interrupted;
            }
            match receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(value) => match value["State"].as_str() {
                    Some("WORKING") => {
                        let working = &value["Working"];
                        progress(
                            working["Progress"].as_f64().unwrap_or(0.0),
                            working["RateAvg"].as_f64().unwrap_or(0.0),
                            working["ETASeconds"].as_u64().unwrap_or(0),
                        );
                    }
                    Some("WORKDONE") => error_code = value["WorkDone"]["Error"].as_i64(),
                    _ => {}
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break child.wait(),
            }
        };

        match status {
            _ if stop_requested() => Encode::Interrupted,
            Ok(status)
                if status.success()
                    && error_code == Some(0)
                    && fs::metadata(output).is_ok_and(|m| m.len() > 0) =>
            {
                Encode::Done
            }
            Ok(status) => Encode::Failed(format!(
                "HandBrakeCLI {status}, error code {}",
                error_code.map_or("none".into(), |code| code.to_string())
            )),
            Err(e) => Encode::Failed(e.to_string()),
        }
    }

    fn open_log(&self, input: &Path) -> io::Result<File> {
        if let Some(dir) = self.log_file.parent() {
            fs::create_dir_all(dir)?;
        }
        if fs::metadata(&self.log_file).is_ok_and(|m| m.len() > 20 << 20) {
            fs::rename(&self.log_file, self.log_file.with_extension("log.1"))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        writeln!(file, "\n===== {} =====", input.display())?;
        Ok(file)
    }
}

fn parse_preset_list(text: &str) -> Vec<String> {
    let mut folders: Vec<&str> = Vec::new();
    let mut presets = Vec::new();
    for line in text.lines() {
        let name = line.trim_start_matches(' ');
        let indent = line.len() - name.len();
        let name = name.trim_end();
        if name.is_empty() || indent % 4 != 0 || indent / 4 > folders.len() {
            continue;
        }
        folders.truncate(indent / 4);
        match name.strip_suffix('/') {
            Some(folder) => folders.push(folder),
            None => presets.push([&folders[..], &[name]].concat().join("/")),
        }
    }
    presets
}
