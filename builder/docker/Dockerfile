FROM rust:latest
WORKDIR /crate
RUN mkdir -p /usr/local/cargo/registry/cache/github.com-1ecc6299db9ec823/hey-crate-cache-is-here
RUN apt-get update && apt-get install -y time pkg-config git build-essential
RUN git config --global user.email "lts@lib.rs"
RUN git config --global user.name "LTS"
RUN rustup component add clippy
RUN rustup toolchain add 1.24.1 && rm -rf ~/.rustup/toolchains/*/share/doc/
RUN rustup toolchain add 1.34.2 && rm -rf ~/.rustup/toolchains/*/share/doc/
RUN rustup toolchain add nightly
RUN rustup toolchain list
RUN cargo install -f lts # 1.5
RUN mkdir -p /tmp/cargo-target-dir /tmp/cargo-prewarm
COPY prewarm_Cargo.toml /tmp/cargo-prewarm/Cargo.toml
ENV CARGO_TARGET_DIR=/tmp/cargo-target-dir
RUN cargo lts "https://github.com/kornelski/crates.io-index"
RUN cargo fetch --manifest-path=/tmp/cargo-prewarm/Cargo.toml
ENV CARGO_MANIFEST_DIR=/crate
COPY run.sh /tmp/run-crate-tests.sh
RUN chmod 0550 /tmp/run-crate-tests.sh

