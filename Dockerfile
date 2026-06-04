FROM rust:slim-bullseye AS builder

WORKDIR /app
COPY . .

RUN cargo build --release

FROM debian:slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/opengate /usr/local/bin/opengate
COPY config/default.toml /etc/opengate/config.toml

EXPOSE 9378

ENTRYPOINT ["./opengate", "serve"]
