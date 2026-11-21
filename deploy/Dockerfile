FROM rust:1.92-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends libssl-dev openssl unzip \
    && rm -rf /var/lib/apt/lists/*

# Use a patched SQLite for WAL and ship its shell for online backups.
RUN curl --fail --silent --show-error --location --retry 3 \
        https://sqlite.org/2026/sqlite-amalgamation-3530400.zip -o /tmp/sqlite.zip \
    && test "$(openssl dgst -sha3-256 -r /tmp/sqlite.zip | awk '{print $1}')" \
        = 628a44cfe82c66aed1ccbbe85a562d2e33ebe64b3288981ed76285612227934e \
    && unzip -q /tmp/sqlite.zip -d /tmp \
    && cd /tmp/sqlite-amalgamation-3530400 \
    && mkdir -p /opt/sqlite/include /opt/sqlite/lib /opt/sqlite/bin \
    && cp sqlite3.h sqlite3ext.h /opt/sqlite/include/ \
    && cc -O2 -fPIC -DSQLITE_THREADSAFE=1 -DSQLITE_ENABLE_COLUMN_METADATA \
        -DSQLITE_ENABLE_UNLOCK_NOTIFY -DSQLITE_USE_URI -c sqlite3.c \
    && ar rcs /opt/sqlite/lib/libsqlite3.a sqlite3.o \
    && cc -O2 shell.c /opt/sqlite/lib/libsqlite3.a -ldl -lpthread -lm -o /opt/sqlite/bin/sqlite3 \
    && /opt/sqlite/bin/sqlite3 -version \
    && rm -rf /tmp/sqlite.zip /tmp/sqlite-amalgamation-3530400

ENV LIBSQLITE3_SYS_USE_PKG_CONFIG=1 \
    SQLITE3_NO_PKG_CONFIG=1 \
    SQLITE3_LIB_DIR=/opt/sqlite/lib \
    SQLITE3_INCLUDE_DIR=/opt/sqlite/include \
    SQLITE3_STATIC=1

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY desktop/Cargo.toml desktop/
COPY interface/Cargo.toml interface/
# Cargo needs target files to resolve the workspace, even for a server-only build.
RUN mkdir -p desktop/src interface/src \
    && touch desktop/src/main.rs interface/src/main.rs
COPY protocol/ protocol/
COPY server/ server/
RUN cargo build --release --locked -p portal-server \
    && ! ldd target/release/portal-server | grep -q libsqlite3

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -m 0700 -o 10001 -g 10001 /data

COPY --from=build /build/target/release/portal-server /usr/local/bin/portal-server
COPY --from=build /opt/sqlite/bin/sqlite3 /usr/local/bin/sqlite3

ENV PORTAL_DB=/data/portal.sqlite
USER 10001:10001
EXPOSE 8765
CMD ["portal-server", "serve", "0.0.0.0:8765"]
