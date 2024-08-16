FROM docker.io/rust:1-slim-bookworm AS build

ARG target=rktpb

WORKDIR /build

COPY . .

RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    cargo build --release; \
    objcopy --compress-debug-sections target/release/$target ./main

################################################################################

FROM docker.io/debian:bookworm-slim

WORKDIR /app

COPY --from=build /build/main /build/Paste.tom[l] ./
COPY --from=build /build/stati[c] ./static
COPY --from=build /build/template[s] ./templates

RUN mkdir upload
ENV PASTE_UPLOAD_DIR=upload
ENV PASTE_ADDRESS=0.0.0.0
ENV PASTE_PORT=8080

CMD ./main
