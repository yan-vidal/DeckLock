# Dependency-only image: the caller's source is mounted only when tests run.
FROM rust:1.93.0-slim-bookworm@sha256:776861219cd851131c1cec3bbd7cbeb16b99a794048097eb69ad9682a8ed0d57 AS rust
FROM ubuntu:24.04@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55

# Rotate apt/native layers each UTC week, including when the recipe is unchanged.
ARG CACHE_EPOCH
RUN test -n "$CACHE_EPOCH" && apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential ca-certificates curl git pkg-config python3 \
    libgtk-4-dev libx11-xcb-dev libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good gstreamer1.0-libav gstreamer1.0-gl \
    libpam0g-dev libwayland-dev wayland-protocols libxkbcommon-dev \
    meson ninja-build xvfb xauth dbus-x11 xfwm4 x11-xserver-utils \
    libxss-dev libxtst6 libxi6 && rm -rf /var/lib/apt/lists/*
COPY --from=rust /usr/local/rustup /usr/local/rustup
COPY --from=rust /usr/local/cargo /usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup PATH=/usr/local/cargo/bin:$PATH
RUN rustup component add --toolchain 1.93.0 rustfmt clippy && \
    rustc --version && cargo --version && cargo fmt --version && cargo clippy --version

COPY bootstrap-native /opt/decklock/scripts/bootstrap-native
RUN /opt/decklock/scripts/bootstrap-native --tests
COPY native-tests-entrypoint /usr/local/bin/native-tests-entrypoint
WORKDIR /workspace
ENTRYPOINT ["/usr/local/bin/native-tests-entrypoint"]
