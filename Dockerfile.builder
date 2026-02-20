FROM rust:1.87-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends zip ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work

