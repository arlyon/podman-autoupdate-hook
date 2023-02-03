FROM docker.io/clux/muslrust as chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef as planner
COPY . .
RUN cargo chef prepare

FROM chef as cacher
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release

FROM chef as builder
COPY . .
COPY --from=cacher /app/target target
RUN cargo build --release

FROM scratch as server
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/podman-autoupdate-hook /podman-autoupdate-hook
CMD ["/podman-autoupdate-hook"]