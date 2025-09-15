FROM rust:1-slim as builder

WORKDIR /usr/src/app

RUN cargo init --bin .
COPY Cargo.toml ./
RUN cargo build --release

COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

FROM debian:stable-slim

ARG OCI_REGISTRY_ROOTDIR=/var/lib/oci-registry
ARG OCI_REGISTRY_PORT=8968

ARG APP_USER=appuser
ARG APP_GROUP=appgroup
ARG UID=1001
ARG GID=1001

RUN groupadd -g $GID $APP_GROUP && \
    useradd -u $UID -g $APP_GROUP -s /bin/sh -m $APP_USER

COPY --from=builder /usr/src/app/target/release/distribution /usr/local/bin/distribution

RUN mkdir -p ${OCI_REGISTRY_ROOTDIR} && chown -R $APP_USER:$APP_GROUP ${OCI_REGISTRY_ROOTDIR}

USER $APP_USER:$APP_GROUP

WORKDIR ${OCI_REGISTRY_ROOTDIR}

EXPOSE ${OCI_REGISTRY_PORT}

CMD ["distribution"]