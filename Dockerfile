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

ARG service_version=unspecified
ARG build_date=unspecified
ARG vcs_ref=unspecified

LABEL maintainer="ffimnsr <ffimnsr@gmail.com>"
LABEL org.label-schema.description="${build_date}"
LABEL org.label-schema.description="A simple Telegram bot with interface to Firefly 3"
LABEL org.label-schema.schema-version="1.0"
LABEL org.label-schema.name="firefly-tg-bot-rs"
LABEL org.label-schema.version="${service_version}"
LABEL org.label-schema.usage="https://github.com/ffimnsr/firefly-tg-bot-rs/blob/master/README.md"
LABEL org.label-schema.url="https://github.com/ffimnsr/firefly-tg-bot-rs"
LABEL org.label-schema.vcs-url="https://github.com/ffimnsr/firefly-tg-bot-rs"
LABEL org.label-schema.vcs-ref="${vcs_ref}"
LABEL org.label-schema.vendor="Where It All Started"
LABEL org.opencontainers.image.source="https://github.com/ffimnsr/firefly-tg-bot-rs"

RUN apt-get update && apt-get install -y libssl1.1 libc6 libgcc1 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /code/target/release/firefly_tg /usr/bin/firefly_tg
EXPOSE 80
ENTRYPOINT [ "/usr/bin/firefly_tg" ]
