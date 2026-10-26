# Exact Rust base plus an immutable Debian package snapshot; no floating apt inputs.
FROM rust:1.99.0-slim-bookworm@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa
RUN rm -f /etc/apt/sources.list.d/debian.sources \
    && printf 'deb [check-valid-until=no] https://snapshot.debian.org/archive/debian/20261025T000000Z/ bookworm main\n' > /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends qemu-system-x86 qemu-system-arm python3 make binutils \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy --toolchain 1.99.0 \
    && rustup target add x86_64-unknown-none aarch64-unknown-none --toolchain 1.99.0
ENV RUSTUP_TOOLCHAIN=1.99.0 CARGO_BUILD_JOBS=2
WORKDIR /workspace
