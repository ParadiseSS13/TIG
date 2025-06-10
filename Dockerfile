# nightly-bookworm-slim
FROM rustlang/rust@sha256:45e423241c20d2cf262a77fa98da909426f93d94def9a5d1ffd333af9870bfe0 AS chef
# We only pay the installation cost once,
# it will be cached from the second build onwards
RUN apt update && apt install -y protobuf-compiler cmake
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release

FROM chef AS builder_win
RUN rustup target add x86_64-pc-windows-gnu
RUN apt update && apt install -y gcc-mingw-w64-x86-64
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --target x86_64-pc-windows-gnu --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --target x86_64-pc-windows-gnu

FROM scratch AS output
WORKDIR /

COPY --from=builder /app/target/release/tig-client /

COPY --from=builder_win /app/target/x86_64-pc-windows-gnu/release/tig-client.exe /
