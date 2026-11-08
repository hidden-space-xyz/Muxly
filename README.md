<p align="center">
  <img
    width="100%"
    alt="Muxly"
    src="https://capsule-render.vercel.app/api?type=waving&height=220&color=0:ffb347,50:ff5e7e,100:8b5cf6&text=Muxly&fontColor=ffffff&fontSize=60&fontAlign=50&fontAlignY=40&animation=fadeIn"
  />
</p>
<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" />
  <img alt="Docker" src="https://img.shields.io/badge/Docker-2496ED?style=for-the-badge&logo=docker&logoColor=white" />
  <img alt="Intel QSV" src="https://img.shields.io/badge/Intel_QSV-0071C5?style=for-the-badge&logo=intel&logoColor=white" />
  <img alt="NVIDIA NVENC" src="https://img.shields.io/badge/NVIDIA_NVENC-76B900?style=for-the-badge&logo=nvidia&logoColor=white" />
  <img alt="AMD VCE" src="https://img.shields.io/badge/AMD_VCE-000000?style=for-the-badge&logo=amd&logoColor=white" />
  <img alt="License" src="https://img.shields.io/badge/GPL--3.0-red?style=for-the-badge&logo=gnu&logoColor=white" />
</p>

<p align="center">
  <img alt="Release" src="https://img.shields.io/github/v/release/hidden-space-xyz/Muxly?style=for-the-badge&label=Release&color=2EA44F&logo=github&logoColor=white" />
  <img alt="CI" src="https://img.shields.io/github/actions/workflow/status/hidden-space-xyz/Muxly/ci.yml?style=for-the-badge&label=CI&logo=githubactions&logoColor=white" />
  <a href="https://github.com/hidden-space-xyz/Muxly/actions/workflows/codeql.yml">
    <img alt="CodeQL" src="https://img.shields.io/github/actions/workflow/status/hidden-space-xyz/Muxly/codeql.yml?branch=develop&style=for-the-badge&label=CodeQL&logo=github&logoColor=white" />
  </a>
</p>

# 🎬 Muxly

**Muxly watches a folder and converts every new video with HandBrake, on the CPU or on an Intel, NVIDIA or
AMD GPU, writing the result to another folder.**

## 📋 What Muxly Does For You

- **📥 Drop and Forget** — Copy videos into one folder and collect the converted files from another
- **📉 Shrink Your Library** — Re-encode recordings and rips with any HandBrake preset, from H.264 to H.265 and AV1
- **💿 Convert Your Discs** — ISO images and DVD or Blu-ray folders become a single video of their main title
- **🔁 Convert Once** — Every video is converted a single time, and again only if it changes
- **🛡️ Keep What You Have** — Nothing in the output folder is ever overwritten

## ⭐ Features

- **🎞️ HandBrake From Source** — `HandBrakeCLI` compiled from its official release on Ubuntu 26.04 LTS, with x265 and Dolby Vision
- **⚡ Hardware Encoding** — Intel QSV, NVIDIA NVENC and NVDEC, and AMD VCE are built in; the preset decides which encoder runs
- **🧩 Your Own Presets** — Use a built-in preset or import the ones you exported from the HandBrake desktop app
- **⏳ Waits for Complete Copies** — A file is processed only after 10 seconds without changing, so half-copied files are left alone
- **📂 Mirrored Folders** — Subfolders under the watched folder are recreated in the output folder
- **🧾 Conversion History** — Every conversion is recorded as done or failed, so restarts never repeat work
- **🧹 Optional Cleanup** — Delete each original, and the folders it leaves empty, once its conversion succeeds
- **👤 Runs as Your User** — Files are written as `PUID:PGID`, which joins the GPU device groups automatically
- **🪶 Tiny Watcher** — A single Rust binary with one dependency: no GUI, no web server, no database

## 🚀 Quick Start

```yaml
services:
  muxly:
    image: ghcr.io/hidden-space-xyz/muxly:latest
    container_name: muxly
    restart: always
    cap_drop:
      - ALL
    cap_add:
      - CHOWN
      - DAC_OVERRIDE
      - KILL
      - SETGID
      - SETUID
    security_opt:
      - no-new-privileges:true
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=Europe/Madrid
      - MUXLY_PRESET=General/Very Fast 1080p30
      - MUXLY_FORMAT=mp4
      - MUXLY_SOURCE_ACTION=keep
    volumes:
      - /path/to/config:/config
      - /path/to/input:/watch
      - /path/to/output:/output
```

```bash
docker compose up -d
docker compose logs -f muxly
```

> [!IMPORTANT]
> The container must start as root, so don't set `user:`: it switches to `PUID:PGID` by itself. The added
> capabilities are only what that needs: taking ownership of `/config`, switching user, and letting `tini`
> forward `docker stop` to the watcher. `/watch` and `/output` must exist and `/output` must be writable by
> `PUID:PGID`, as must `/watch` when sources are deleted.

To encode on the GPU, see [Hardware Encoding](#-hardware-encoding).

## 🔄 How It Works

- **🔍 Scan** — Every 10 seconds Muxly looks for videos under `/watch` (`mkv mp4 m4v avi mov wmv flv webm ts m2ts mts mpg mpeg vob ogv 3gp iso`), oldest first. Names starting with `.` are ignored
- **💿 Discs** — ISO images and folders containing `VIDEO_TS` or `BDMV` are treated as one disc, and only their main title is converted
- **⏳ Wait** — A video is picked up once its size and modification time have stayed the same for 10 seconds
- **🎞️ Convert** — One video at a time, to `/output` under the same relative path with the new extension. HandBrake writes into a hidden `/output/.muxly-tmp` folder and the finished file is moved into place
- **🛡️ Never Overwrite** — If the output file already exists, the video waits in `/watch` until that name is free
- **🔁 Retry on Change** — A video that changes while it is being converted is converted again from scratch
- **🧾 Record** — The result is written to `history.tsv`. A video in the history is skipped until it changes, or until you delete its line
- **🧹 Clean Up** — With `MUXLY_SOURCE_ACTION=delete`, the original and the folders it leaves empty under `/watch` are removed after a successful conversion

On `docker stop`, the conversion in progress is cancelled and its partial file is discarded; it starts over on
the next run.

## ⚙️ Configuration

| Variable              | Values              | Default                     | Effect                                                                       |
| --------------------- | ------------------- | --------------------------- | ---------------------------------------------------------------------------- |
| `PUID` / `PGID`       | User and group IDs  | `1000`                      | User and group Muxly runs as; the groups of the `/dev/dri` devices are added |
| `TZ`                  | IANA time zone      | `Etc/UTC`                   | Container time zone, used by the timestamps in `logs/handbrake.log`          |
| `MUXLY_PRESET`        | `<category>/<name>` | `General/Very Fast 1080p30` | HandBrake preset, given with its category or by its name alone               |
| `MUXLY_FORMAT`        | `mp4`, `mkv`        | `mp4`                       | Container format of the converted files                                      |
| `MUXLY_SOURCE_ACTION` | `keep`, `delete`    | `keep`                      | What happens to the original after a successful conversion                   |

The preset is checked at startup, and Muxly refuses to start with one that doesn't exist. List them with:

```bash
docker run --rm ghcr.io/hidden-space-xyz/muxly HandBrakeCLI --preset-list
```

### Files in `/config`

| File                 | Purpose                                                                                      |
| -------------------- | -------------------------------------------------------------------------------------------- |
| `presets.json`       | Optional presets exported from the HandBrake desktop app, available to `MUXLY_PRESET`        |
| `history.tsv`        | Videos already processed, `done` or `failed`. Delete a line to convert that video again      |
| `logs/handbrake.log` | HandBrake's output for every conversion, rotated to `handbrake.log.1` once it exceeds 20 MiB |

> [!NOTE]
> A failed conversion is not retried by itself: check `logs/handbrake.log`, then delete its line from
> `history.tsv` or replace the file.

## ⚡ Hardware Encoding

HandBrake chooses the encoder from the preset, so encoding on the GPU only takes a preset that uses it (the
`Hardware` category, or your own in `presets.json`) and access to the device:

| Encoder         | Example preset               | The container needs                  |
| --------------- | ---------------------------- | ------------------------------------ |
| 🔵 Intel QSV    | `Hardware/H.265 QSV 1080p`   | `/dev/dri`                           |
| 🟢 NVIDIA NVENC | `Hardware/H.265 NVENC 1080p` | NVIDIA Container Toolkit on the host |
| 🔴 AMD VCE      | `Hardware/H.265 VCN 1080p`   | `/dev/dri` and AMD's AMF runtime     |

At startup the log shows what HandBrake detected; AMD appears as `vcn`:

```text
[init] nvenc: version 13.1 is available
[init] nvdec: is available
[init] vcn: not available on this system
[init] qsv: not available on this system
```

### Intel QSV

Add the device to the `muxly` service:

```yaml
devices:
  - /dev/dri:/dev/dri
```

Works on 11th generation Intel processors or later and on Intel Arc, with the drivers Ubuntu packages.

### NVIDIA NVENC

With the [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html)
installed on the host, reserve the GPU in the `muxly` service:

```yaml
deploy:
  resources:
    reservations:
      devices:
        - driver: nvidia
          count: all
          capabilities: [gpu]
```

With `docker run`, use `--gpus all`. The image sets `NVIDIA_DRIVER_CAPABILITIES=compute,video,utility`, so the
toolkit mounts the driver's encoding and decoding libraries.

### AMD VCE

Add the device to the `muxly` service:

```yaml
devices:
  - /dev/dri:/dev/dri
```

> [!WARNING]
> On Linux, HandBrake reaches the AMD encoder through AMF, which needs AMD's proprietary AMF runtime
> (`libamfrt64.so.1`). Ubuntu does not package it, so the image does not include it, and VCE presets fail
> without it.

## 🛠️ Building from Source

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) with BuildKit, for the image
- A stable Rust toolchain, only to work on the watcher outside Docker

### Build the image

```bash
git clone https://github.com/hidden-space-xyz/Muxly.git
cd Muxly
docker build -t muxly .
```

HandBrake is compiled from source, so the first build takes a while. Two build arguments change what goes in:

| Build argument             | Default | Effect                                                                          |
| -------------------------- | ------- | ------------------------------------------------------------------------------- |
| `HANDBRAKE_ENABLE_LIBDOVI` | `1`     | `0` drops Dolby Vision support                                                  |
| `HANDBRAKE_ENABLE_FDK_AAC` | `0`     | `1` adds the fdk-aac encoder, whose license makes the image non-redistributable |

```bash
docker build --build-arg HANDBRAKE_ENABLE_LIBDOVI=0 -t muxly .
```

### Work on the watcher

```bash
cargo build --release
cargo clippy --all-targets
cargo fmt --check
```

Outside Docker the binary needs `HandBrakeCLI` on `PATH` and the fixed folders `/watch`, `/output` and
`/config`.

## 📁 Repository

```text
Muxly/
├── src/
│   ├── main.rs           Polling loop, conversion pipeline and signal handling
│   ├── config.rs         Environment variables and folder checks
│   ├── watch.rs          Folder scan, disc detection, fingerprints and history
│   └── handbrake.rs      HandBrakeCLI invocation, progress, log and preset list
├── rootfs/usr/local/bin/
│   └── entrypoint.sh     Drops to PUID:PGID, joins the GPU groups and logs hardware detection
├── Dockerfile            HandBrake from source, the watcher and the runtime image
└── .github/              CI, image publishing and CodeQL
```

The image runs `tini` as PID 1, which starts the entrypoint and then the watcher. HandBrake's runtime
packages are not listed by hand: the build derives them from the libraries `HandBrakeCLI` links against, and
fails if any of them is missing from the final image. See [AGENTS.md](AGENTS.md) for the full layout.

## 💡 How to Contribute

Contributions are welcome from everyone, regardless of technical background.

### For non-developers

- **Report Bugs**: Found something that doesn't work? Open an issue with your GPU, the preset and the relevant part of `logs/handbrake.log`.
- **Suggest Features**: Ideas for new features or improvements are welcome as issues.
- **Hardware Testing**: Reports from GPUs and drivers not tested yet are especially useful.
- **Documentation**: Improve or clarify the documentation.
- **Spread the Word**: Share the project or tell your friends.

### For developers

1. Fork the repository.
2. Create a feature branch from `develop`.
3. Implement your changes, keeping `README.md` in sync with the configuration.
4. Open a pull request against `develop`; `master` only receives release merges.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/). The release workflow
derives the next version from every commit since the latest stable tag: a breaking `!` bumps the major
version, `feat:` bumps minor, and `fix:` or `perf:` bumps patch. Other commit types do not publish a release
by themselves.

CI checks the watcher with the Rust toolchain Ubuntu 26.04 packages, the same one the image builds with. Run
its local equivalents, and build the image, before opening a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
docker build -t muxly .
```

### For maintainers

Landing a releasable Conventional Commit on `master` checks formatting, lints and builds the watcher, then
builds the image with provenance and an SBOM, pushes it to `ghcr.io/hidden-space-xyz/muxly:<version>`,
promotes it to `latest` and creates the `vX.Y.Z` tag and GitHub release. No version is stored in the source.
CodeQL scans `develop` and `master` independently and does not block releases. Dependencies are updated by
hand: crates in `Cargo.toml`, actions in the workflows, the Ubuntu base image, and HandBrake through
`ARG HANDBRAKE_VERSION` in the `Dockerfile`.

## 📜 License

Muxly is released under the [GNU General Public License v3.0](LICENSE). You are free to use, study, modify and
redistribute it, and any derivative work must carry the same license. HandBrake is distributed under the GNU
General Public License v2.0.

<p align="center">
  <img
    width="100%"
    alt=""
    src="https://capsule-render.vercel.app/api?type=waving&section=footer&height=140&color=0:8b5cf6,50:ff5e7e,100:ffb347&animation=twinkling"
  />
</p>
