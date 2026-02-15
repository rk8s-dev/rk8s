FROM rust:1-slim AS builder

ARG PROFILE=debug

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN if [ "$PROFILE" = "release" ]; then \
        cargo build -p distribution --release; \
    else \
        cargo build -p distribution; \
    fi

FROM debian:stable-slim

ARG PROFILE=debug
ARG OCI_REGISTRY_ROOTDIR=/var/lib/oci-registry
ARG OCI_REGISTRY_PORT=8968

ARG APP_USER=appuser
ARG APP_GROUP=appgroup
ARG UID=1001
ARG GID=1001

RUN groupadd -g $GID $APP_GROUP && \
    useradd -u $UID -g $APP_GROUP -s /bin/sh -m $APP_USER

COPY --from=builder /app/target/ /tmp/target/
RUN if [ "$PROFILE" = "release" ]; then \
        cp /tmp/target/release/distribution /usr/local/bin/distribution; \
    else \
        cp /tmp/target/debug/distribution /usr/local/bin/distribution; \
    fi && \
    chmod +x /usr/local/bin/distribution

RUN mkdir -p ${OCI_REGISTRY_ROOTDIR} && chown -R $APP_USER:$APP_GROUP ${OCI_REGISTRY_ROOTDIR}

USER $APP_USER:$APP_GROUP

WORKDIR ${OCI_REGISTRY_ROOTDIR}

EXPOSE ${OCI_REGISTRY_PORT}

CMD ["distribution"]
