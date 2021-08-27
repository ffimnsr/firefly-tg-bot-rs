# The cache layer.
FROM rust:1.54.0 AS base

ENV USER=root

WORKDIR /code
COPY . /code
RUN mkdir -p .cargo \
    && cargo fetch \
    && cargo vendor >> .cargo/config.toml

CMD [ "cargo", "test", "--offline" ]

# The builder.
FROM base AS builder

RUN rustup component add rustfmt
RUN cargo build --release --offline

# The final build.
FROM debian:buster-slim

RUN apt-get update && apt-get install -y libssl1.1 libc6 libgcc1 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /code/target/release/firefly_tg /usr/bin/firefly_tg
EXPOSE 80
ENTRYPOINT [ "/usr/bin/firefly_tg" ]
