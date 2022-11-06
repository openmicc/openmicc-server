FROM rust:1.64-buster

# Update crates.io index
# https://github.com/rust-lang/cargo/issues/3377#issuecomment-410169587
RUN cargo search --limit 0

RUN apt-get update

# Install OS dependencies
RUN apt-get install -y python3-pip

# ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
# EXPENSIVE, try not to break cache
# ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

# Create shell package
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs

# download cargo dependencies
RUN cargo fetch


RUN apt-get install -y libzstd-dev

# # build deps first
RUN cat src/main.rs
RUN cargo build --release

# Then build the app
RUN rm -r src
COPY . .
# RUN cargo check
RUN cargo build --release

# TODO: Producition builds
CMD ["./target/release/openmicc-server", "8000"]
