use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

const EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "wmv", "flv", "webm", "ts", "m2ts", "mts", "mpg", "mpeg",
    "vob", "ogv", "3gp", "iso",
];
const STABLE_TIME: Duration = Duration::from_secs(10);

pub struct Candidate {
    pub path: PathBuf,
    pub size: u64,
    pub modified: u128,
}

impl Candidate {
    pub fn fingerprint(&self) -> u64 {
        let bytes = self
            .path
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .copied()
            .chain([0])
            .chain(self.size.to_le_bytes())
            .chain(self.modified.to_le_bytes());
        bytes.fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
    }
}

pub fn candidates(root: &Path) -> Vec<Candidate> {
    let mut found = Vec::new();
    walk(root, &mut found);
    found.sort_by_key(|c| c.modified);
    found
}

fn walk(dir: &Path, found: &mut Vec<Candidate>) {
    let Ok(entries) = fs::read_dir(dir) else {
        warn!("cannot read '{}'", dir.display());
        return;
    };
    for entry in entries.flatten() {
        let (path, name) = (entry.path(), entry.file_name());
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if name.to_string_lossy().starts_with('.') || (kind.is_dir() && is_disc_folder(&name)) {
            continue;
        }
        let is_source = if kind.is_dir() {
            contains_disc_folder(&path)
        } else {
            accepts(&path)
        };
        if is_source {
            match snapshot(&path) {
                Ok((size, modified)) if size > 0 => found.push(Candidate {
                    path,
                    size,
                    modified,
                }),
                Ok(_) => {}
                Err(e) => warn!("cannot read '{}': {e}", path.display()),
            }
        } else if kind.is_dir() {
            walk(&path, found);
        }
    }
}

fn accepts(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

fn is_disc_folder(name: &OsStr) -> bool {
    name.eq_ignore_ascii_case("VIDEO_TS") || name.eq_ignore_ascii_case("BDMV")
}

fn contains_disc_folder(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|mut entries| {
        entries.any(|e| e.is_ok_and(|e| e.path().is_dir() && is_disc_folder(&e.file_name())))
    })
}

pub fn snapshot(path: &Path) -> io::Result<(u64, u128)> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() {
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        return Ok((metadata.len(), modified));
    }
    fs::read_dir(path)?.try_fold((0, 0), |(size, modified), entry| {
        let (s, m) = snapshot(&entry?.path())?;
        Ok((size + s, modified.max(m)))
    })
}

#[derive(Default)]
pub struct Tracker {
    seen: HashMap<PathBuf, (u64, u128, Instant)>,
    blocked: HashSet<PathBuf>,
}

impl Tracker {
    pub fn is_stable(&mut self, candidate: &Candidate) -> bool {
        match self.seen.get(&candidate.path) {
            Some(&(size, modified, since))
                if (size, modified) == (candidate.size, candidate.modified) =>
            {
                since.elapsed() >= STABLE_TIME
            }
            _ => {
                self.seen.insert(
                    candidate.path.clone(),
                    (candidate.size, candidate.modified, Instant::now()),
                );
                false
            }
        }
    }

    pub fn block(&mut self, path: &Path) -> bool {
        self.blocked.insert(path.to_path_buf())
    }

    pub fn forget(&mut self, path: &Path) {
        self.seen.remove(path);
        self.blocked.remove(path);
    }

    pub fn keep_only(&mut self, candidates: &[Candidate]) {
        let present: HashSet<&Path> = candidates.iter().map(|c| c.path.as_path()).collect();
        self.seen.retain(|path, _| present.contains(path.as_path()));
        self.blocked.retain(|path| present.contains(path.as_path()));
    }
}

pub struct History {
    path: PathBuf,
    seen: HashSet<u64>,
}

impl History {
    pub fn load(path: PathBuf) -> io::Result<Self> {
        let seen = match fs::read_to_string(&path) {
            Ok(text) => text
                .lines()
                .filter_map(|line| u64::from_str_radix(line.split('\t').next()?, 16).ok())
                .collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => HashSet::new(),
            Err(e) => return Err(e),
        };
        Ok(History { path, seen })
    }

    pub fn contains(&self, fingerprint: u64) -> bool {
        self.seen.contains(&fingerprint)
    }

    pub fn record(&mut self, fingerprint: u64, done: bool, source: &Path) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let status = if done { "done" } else { "failed" };
        writeln!(file, "{fingerprint:016x}\t{status}\t{}", source.display())?;
        self.seen.insert(fingerprint);
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
