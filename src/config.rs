use std::path::{Path, PathBuf};
use std::{env, fs, io};

pub struct Config {
    pub watch_dir: PathBuf,
    pub output_dir: PathBuf,
    pub config_dir: PathBuf,
    pub preset: String,
    pub extension: &'static str,
    pub delete_source: bool,
}

fn var(name: &str) -> Option<String> {
    env::var(format!("MUXLY_{name}"))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn choice(name: &str, options: [&'static str; 2]) -> Result<&'static str, String> {
    let Some(raw) = var(name) else {
        return Ok(options[0]);
    };
    options
        .into_iter()
        .find(|option| option.eq_ignore_ascii_case(&raw))
        .ok_or_else(|| {
            format!(
                "invalid value '{raw}' for MUXLY_{name}, expected '{}' or '{}'",
                options[0], options[1]
            )
        })
}

fn writable(dir: &Path) -> io::Result<()> {
    let probe = dir.join(format!(".muxly-{}", std::process::id()));
    fs::File::create_new(&probe)?;
    fs::remove_file(probe)
}

impl Config {
    pub fn load() -> Result<Self, String> {
        Ok(Config {
            watch_dir: "/watch".into(),
            output_dir: "/output".into(),
            config_dir: "/config".into(),
            preset: var("PRESET").unwrap_or("General/Very Fast 1080p30".into()),
            extension: choice("FORMAT", ["mp4", "mkv"])?,
            delete_source: choice("SOURCE_ACTION", ["keep", "delete"])? == "delete",
        })
    }

    pub fn check_paths(&self) -> Result<(), String> {
        for (path, kind, must_write) in [
            (&self.watch_dir, "watch", self.delete_source),
            (&self.output_dir, "output", true),
        ] {
            if !path.is_dir() {
                return Err(format!("{kind} folder '{}' doesn't exist", path.display()));
            }
            if must_write {
                writable(path).map_err(|e| {
                    format!("{kind} folder '{}' is not writable: {e}", path.display())
                })?;
            }
        }
        Ok(())
    }
}
