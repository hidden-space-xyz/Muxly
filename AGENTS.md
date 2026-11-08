# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Overview

Muxly is a Docker image that watches a folder and converts every new video with `HandBrakeCLI`. It has two parts: a `Dockerfile` that compiles HandBrake from source on Ubuntu 26.04 with the Intel QSV, NVIDIA NVENC/NVDEC and AMD VCE hardware encoders, and a small Rust binary (`src/`) that does the watching. No GPU vendor is the default: the preset chooses the encoder, and the README documents each vendor on equal terms. `README.md` is the user documentation (in English) and holds the table of environment variables and defaults; keep it in sync when configuration changes. It follows the house style of the other `hidden-space-xyz` projects (BackupZCrypt, Qrypt): a capsule-render banner and footer, shields.io badges, emoji section headings and Prettier-aligned tables. Code, log messages and error strings are in English.

## Commands

```bash
cargo build --release
```

```bash
cargo clippy --all-targets --locked -- -D warnings
```

```bash
cargo fmt --check
```

There are no tests yet; once they exist, `cargo test <name>` runs a single one.

```bash
docker build -t muxly .
```

```bash
docker run --rm muxly HandBrakeCLI --preset-list
```

The image build compiles HandBrake from source, so it is slow. Build options: `--build-arg HANDBRAKE_ENABLE_LIBDOVI=0` drops Dolby Vision; `--build-arg HANDBRAKE_ENABLE_FDK_AAC=1` adds fdk-aac, whose license makes the image non-redistributable.

Running the binary outside Docker needs `HandBrakeCLI` on `PATH` (startup fails without it, because it lists presets first) and the fixed folders `/watch`, `/output` (writable) and `/config`; their paths are not configurable.

The configuration is deliberately minimal: `PUID`, `PGID`, `TZ`, `MUXLY_PRESET`, `MUXLY_FORMAT` (`mp4`/`mkv`), `MUXLY_OUTPUT_LAYOUT` (`flat`/`mirror`), `MUXLY_SOURCE_ACTION` (`keep`/`delete`), `MUXLY_OTHER_ACTION` (`keep`/`delete`) and `MUXLY_OTHER_EXCEPTIONS` (extension list), plus the optional `/config/presets.json`. Scan interval, stability time, processed extensions and umask are constants.

## Architecture

### Image (`Dockerfile`, `rootfs/`)

Three stages on `ubuntu:26.04`, after the `# syntax=docker/dockerfile:1` directive:

1. `build` compiles HandBrake (`--disable-gtk --enable-x265 --enable-qsv --enable-nvenc --enable-nvdec --enable-vce`) into `/out` (`clang` is a build dependency because NVDEC makes FFmpeg require `cuda_llvm`), then derives its runtime apt packages by running `ldd` on `HandBrakeCLI` and mapping each library to its package with `dpkg -S` into `/runtime-packages.txt`. There is no hand-maintained runtime package list.
2. `muxly` builds the Rust binary with Ubuntu's packaged `cargo` (`--locked`), so the toolchain is whatever Ubuntu 26.04 ships, not rustup.
3. `runtime` installs those packages plus the Intel media driver, copies `/out`, the binary and `rootfs/`, and fails the build if `ldd` reports a missing library. NVENC, NVDEC and VCE load the vendor driver libraries at runtime, so they add no packages: the NVIDIA Container Toolkit mounts NVIDIA's (the image sets `NVIDIA_DRIVER_CAPABILITIES=compute,video,utility`), and AMD's proprietary AMF runtime is not included.

`tini -g` is PID 1 and runs `rootfs/usr/local/bin/entrypoint.sh`, which exits with an error unless it runs as root, sets umask `0022`, drops to `PUID:PGID` with `setpriv` (adding the group of every device under `/dev/dri`), chowns `/config`, warns about render nodes that user cannot open, sets `HOME` to `/config`, logs HandBrake's own hardware detection (the `nvenc`/`nvdec`/`vcn`/`qsv` lines `HandBrakeCLI --version` prints to stderr) and execs `muxly`. The README's Compose example drops every capability and adds back only `CHOWN`, `DAC_OVERRIDE` (chowning `/config` entries owned by another user, even `0700` ones), `SETUID`/`SETGID` (`setpriv`) and `KILL` (without it, `tini` as root cannot forward `SIGTERM` to the `PUID` process and dies with `Operation not permitted`); it was verified end to end with `no-new-privileges`. Keep that list in step with the entrypoint.

### Binary (`src/`)

- `config.rs`: `Config` holds the fixed folders and the `MUXLY_*` variables; `choice()` parses the two-valued ones case-insensitively, the first value being the default. `MUXLY_OTHER_EXCEPTIONS` is split on commas and whitespace, lowercased and stripped of a leading `.`; `is_exception` matches each entry against the end of the file name after a `.`, so `rar` covers `x.part1.rar` and `tar.gz` works. `check_paths` checks that `/watch` and `/output` exist and probes writability of `/output`, and of `/watch` when sources or other files are deleted.
- `main.rs`: single-threaded polling loop. Each pass `poll` first deletes, with `MUXLY_OTHER_ACTION=delete`, the stable non-video files that are not exceptions (a failed deletion is logged once, through `Tracker::block`), then converts at most one stable candidate; after a conversion the loop rescans at once, otherwise it sleeps `SCAN_INTERVAL` (10 s). Existing outputs are never overwritten: a candidate whose target exists is skipped without being recorded (warned once, through `Tracker::block`) and converted once the name is free; if the target appears during the encode, the result is discarded the same way. `output_name` keeps the path relative to `/watch` with `mirror` and only the file or disc folder name with `flat` (the default), so same-named videos in different folders wait for each other. HandBrake writes to `/output/.muxly-tmp/<fingerprint>.<ext>` and the result is moved into place (rename, falling back to copy + delete across filesystems). If the source changed during the encode, the result is discarded and not recorded, so it is converted again. After success, with `delete`, the source is removed. Every deletion, of a source or of another file, is followed by `remove_empty_parents`, which removes the parent folders it left empty up to `/watch`; folders that were already empty are left alone. SIGINT/SIGTERM are caught through a raw `extern "C" signal` binding (no `libc` crate) that sets the `STOP` flag; loops check `stop_requested()` and the HandBrake child is killed. The `info!`/`warn!`/`error!` macros are defined at the top of `main.rs`, before the `mod` declarations, so the submodules can use them.
- `watch.rs`: `scan` walks `/watch` recursively into a `Scan` with the videos (files with an extension in `EXTENSIONS`, oldest mtime first) and, only when other files are deleted, every other regular file (symbolic links and special files are never collected). Names starting with `.` are skipped, which hides `.muxly-tmp`, the writability probes and other tools' temporary files. A folder containing `VIDEO_TS` or `BDMV` is a single candidate (a disc) whose size and mtime are aggregated recursively, and is never walked, so its files are never treated as other files. A candidate's fingerprint is FNV-1a over path, size and mtime; `History` is the append-only `history.tsv` (`<fingerprint hex>\t<done|failed>\t<path>`), and any fingerprint in it is skipped, so a file is retried only if it changes or its line is deleted. `Tracker` keeps in memory how long each candidate or other file has been unchanged (`STABLE_TIME`, 10 s) and which ones were already warned about (waiting for a free output name, or failing to be deleted).
- `handbrake.rs`: runs `HandBrakeCLI --json -i … -o … --main-feature --preset … --format av_<mp4|mkv>`, importing `/config/presets.json` when present. A reader thread extracts the `Progress: { … }` JSON blocks from stdout; stderr is appended to `logs/handbrake.log`, rotated to `.log.1` above 20 MiB. An encode succeeds only if the exit status is success, `WorkDone.Error` is 0 and the output is non-empty. At startup the preset is validated against the parsed `--preset-list` output (as `Category/Name` or just `Name`); validation is skipped if parsing finds no presets.

The only crate dependency is `serde_json`.

## Release and maintenance

The workflows follow the same scheme as the other `hidden-space-xyz` projects: work lands on `develop` and `master` only receives release merges.

- CI (`.github/workflows/ci.yml`) runs only on pushes to `master`, never on pull requests or `develop`. Its `quality` job runs in an `ubuntu:26.04` container with Ubuntu's `cargo`, `rust-clippy` and `rustfmt`, the same toolchain the image builds with, and runs `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo build --release --locked`. Runs are serialized without cancelling one in progress.
- The `release` job calls the reusable `.github/workflows/docker-publish.yml`. `.github/scripts/release-version.mjs` takes the highest stable `vX.Y.Z` tag (or `0.0.0`) and the largest Conventional Commit increment since it: `type!:` major, `feat:` minor, `fix:`/`perf:` patch, anything else (`refactor`, `docs`, `chore`…) none. With no increment nothing is built. Otherwise the image is built for linux/amd64 with provenance and SBOM and pushed to `ghcr.io/hidden-space-xyz/muxly:<X.Y.Z>`; only after the push succeeds and the version is rechecked does the last job promote it to `latest` and create the `vX.Y.Z` tag and GitHub release. The previous tag must be an ancestor of `HEAD`, so rerunning an old commit cannot move `latest` backwards. No version is stored in the source: `Cargo.toml` has none, so the crate is `0.0.0`.
- CodeQL (`.github/workflows/codeql.yml`) analyzes the Rust code (build mode `none`) on pushes to `develop` and `master` that touch `src/`, `Cargo.toml`, `Cargo.lock` or the workflow. It does not gate releases, and findings never fail the run.
- Dependencies are updated by hand: crates, GitHub Actions, the Ubuntu base image and HandBrake (`ARG HANDBRAKE_VERSION` in the `Dockerfile`). There is no Dependabot or Renovate, and none should be added.
- `.gitattributes` forces LF on every file; `entrypoint.sh` must keep LF endings to run in the container.
