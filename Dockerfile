# syntax=docker/dockerfile:1

ARG HANDBRAKE_VERSION=1.11.2

FROM ubuntu:26.04 AS build

ARG HANDBRAKE_VERSION
ARG HANDBRAKE_URL=https://github.com/HandBrake/HandBrake/releases/download/${HANDBRAKE_VERSION}/HandBrake-${HANDBRAKE_VERSION}-source.tar.bz2
ARG HANDBRAKE_ENABLE_FDK_AAC=0
ARG HANDBRAKE_ENABLE_LIBDOVI=1
ARG DEBIAN_FRONTEND=noninteractive

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update \
    && apt-get install -y \
        autoconf automake build-essential cmake git libass-dev libbz2-dev libfontconfig-dev libfreetype-dev \
        libfribidi-dev libharfbuzz-dev libjansson-dev liblzma-dev libmp3lame-dev libnuma-dev libogg-dev libopus-dev \
        libsamplerate0-dev libspeex-dev libssl-dev libtheora-dev libtool libtool-bin libturbojpeg0-dev libvorbis-dev libvpx-dev \
        libx11-dev libx264-dev libxml2-dev m4 make meson nasm ninja-build patch pkg-config zlib1g-dev \
        libva-dev libdrm-dev clang \
        cargo cargo-c \
        bzip2 ca-certificates curl python3 xz-utils \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

RUN curl -fsSL "$HANDBRAKE_URL" | tar -xj --strip-components=1

RUN CONFIGURE_FLAGS=(--prefix=/usr --disable-gtk --enable-x265 --enable-qsv --enable-nvenc --enable-nvdec --enable-vce) \
    && if [ "$HANDBRAKE_ENABLE_FDK_AAC" = "1" ]; then CONFIGURE_FLAGS+=(--enable-fdk-aac); fi \
    && if [ "$HANDBRAKE_ENABLE_LIBDOVI" = "1" ]; then CONFIGURE_FLAGS+=(--enable-libdovi); else CONFIGURE_FLAGS+=(--disable-libdovi); fi \
    && ./configure "${CONFIGURE_FLAGS[@]}" \
    && make -C build -j"$(nproc)" \
    && make -C build install DESTDIR=/out

RUN for lib in $(ldd /out/usr/bin/HandBrakeCLI | awk '$2 == "=>" && $3 ~ /^\// {print $3}'); do \
        real="$(readlink -f "$lib")"; \
        dpkg -S "$real" 2>/dev/null || dpkg -S "*/$(basename "$real")" 2>/dev/null || { echo "No package owns $real" >&2; exit 1; }; \
    done \
    | grep -v '^diversion ' \
    | sed -E 's/: \/.*$//' | tr ',' '\n' | sed -E 's/^ +//; s/:.*$//' \
    | sort -u > /runtime-packages.txt \
    && cat /runtime-packages.txt

FROM ubuntu:26.04 AS muxly

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates cargo \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /muxly

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM ubuntu:26.04 AS runtime

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        intel-media-va-driver-non-free libmfx-gen1.2 \
        tini tzdata fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /runtime-packages.txt /tmp/runtime-packages.txt

RUN apt-get update \
    && xargs -a /tmp/runtime-packages.txt apt-get install -y --no-install-recommends \
    && rm -rf /var/lib/apt/lists/* /tmp/runtime-packages.txt

COPY --from=build /out/ /
COPY --from=muxly /muxly/target/release/muxly /usr/local/bin/muxly
COPY rootfs/ /

RUN ! ldd /usr/bin/HandBrakeCLI | grep 'not found' \
    && HandBrakeCLI --version

ENV TZ=Etc/UTC \
    NVIDIA_DRIVER_CAPABILITIES=compute,video,utility \
    PUID=1000 \
    PGID=1000

VOLUME ["/config", "/watch", "/output"]

ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["/usr/local/bin/muxly"]
