FROM rust:latest
WORKDIR /crate
RUN apt-get update && apt-get install -y time pkg-config git build-essential
RUN git config --global user.email "lts@lib.rs"
RUN git config --global user.name "LTS"
RUN rustup component add clippy
RUN rustup toolchain add 1.24.1
RUN rustup toolchain add 1.34.2
RUN rustup toolchain list
RUN cargo install -f lts # 12

RUN mkdir -p /tmp/cargo-target-dir /tmp/cargo-prewarm
COPY prewarm_Cargo.toml /tmp/cargo-prewarm/Cargo.toml
ENV CARGO_TARGET_DIR=/tmp/cargo-target-dir
RUN CARGO_MANIFEST_DIR=/tmp/cargo-prewarm cargo lts 1.34.2
RUN cargo fetch --manifest-path=/tmp/cargo-prewarm/Cargo.toml
RUN CARGO_MANIFEST_DIR=/tmp/cargo-prewarm cargo lts 1.24.1
RUN rm /tmp/cargo-prewarm/Cargo.lock
RUN cargo fetch --manifest-path=/tmp/cargo-prewarm/Cargo.toml
ENV CARGO_MANIFEST_DIR=/crate
COPY run.sh /tmp/run-crate-tests.sh
RUN chmod 0550 /tmp/run-crate-tests.sh

ARG crate_tarball
RUN test -n "$crate_tarball"
COPY ${crate_tarball} /tmp/crate.tar.gz
RUN tar -xvz --one-file-system --no-acls --no-same-permissions --no-same-owner --exclude-vcs --group=nobody --no-wildcards --exclude='.*' --exclude='*/.*' --no-xattrs --strip-components=1 -f /tmp/crate.tar.gz
RUN rm -rf rust-toolchain Cargo.lock .cargo
RUN ls -lAR
ARG deps_date
RUN cargo lts ${deps_date}
RUN cargo generate-lockfile
RUN cargo fetch --locked
