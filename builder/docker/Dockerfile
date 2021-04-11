FROM rustops/crates-build-env
RUN useradd -u 4321 --create-home --user-group -s /bin/bash rustyuser
RUN chown -R 4321:4321 /home/rustyuser
USER rustyuser
WORKDIR /home/rustyuser
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.51.0 --verbose # wat
ENV PATH="$PATH:/home/rustyuser/.cargo/bin"
RUN rustup set profile minimal
RUN rustup toolchain add 1.29.2
RUN rustup toolchain add 1.36.0
RUN rustup toolchain add 1.41.1
RUN rustup toolchain add 1.51.0
RUN rustup toolchain list

