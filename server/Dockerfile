FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home-dir /data st \
    && mkdir -p /data && chown st /data
COPY --from=build /src/target/release/sillytavern-rs-server /usr/local/bin/sillytavern-rs-server
USER st
ENV ST_SYNC_DATA=/data ST_SYNC_ADDR=0.0.0.0:8787
VOLUME /data
EXPOSE 8787
CMD ["sillytavern-rs-server"]
