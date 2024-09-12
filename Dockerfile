# syntax=docker/dockerfile:latest

FROM oven/bun:alpine AS client
WORKDIR /app

COPY --link package.json ./
RUN bun install

COPY --link tsconfig.json rsbuild.config.ts ./
COPY --link src/client ./src/client
COPY --link src/games ./src/games
ARG SITE_URL
RUN SITE_URL=$SITE_URL bun run build


FROM rust:alpine AS server
WORKDIR /app

RUN apk add --no-cache musl-dev

COPY --link Cargo.toml build.rs ./
COPY --link src/server ./src/server
COPY --link src/games ./src/games
ENV SKIP_CLIENT_BUILD=1

RUN --mount=type=cache,target=/usr/local/cargo/registry \
	--mount=type=cache,target=/app/target <<-EOF
	set -eu
	cargo build --release
	cp target/release/clickraids /clickraids
EOF


FROM alpine:latest
WORKDIR /app

RUN apk add --no-cache ca-certificates

COPY --from=server /clickraids ./clickraids
COPY --from=client /app/public ./public

ENV STATE_DIR=/data \
	BACKEND_PORT=8000

VOLUME ["/data"]
EXPOSE 8000

CMD ["./clickraids"]
