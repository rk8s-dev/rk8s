FROM debian:trixie-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        fuse3 \
        libsqlite3-0 \
        libssl3 \
        procps \
        tini \
        util-linux \
        xfsprogs \
        acl \
        attr \
        bc \
        dbench \
        dump \
        e2fsprogs \
        fio \
        liburing2 \
        gawk \
        libuuid1 \
        lvm2 \
        make \
        perl \
        psmisc \
        python3 \
        quota \
        sed \
        sudo \
        uuid-runtime \
        xfsdump \
        exfatprogs \
        f2fs-tools \
        udftools && \
    rm -rf /var/lib/apt/lists/*

COPY target/release/slayerfs /usr/local/bin/slayerfs
COPY slayerfs/tests/scripts/xfstests-prebuilt/xfstests-prebuilt.tar.gz /opt/xfstests-prebuilt.tar.gz
COPY slayerfs/tests/scripts/xfstests_slayer.exclude /opt/xfstests_slayer.exclude
COPY slayerfs/docker/xfstests_report.sh /usr/local/bin/xfstests_report.sh
COPY slayerfs/docker/compose-xfstests/run_xfstests_in_container.sh /usr/local/bin/run_xfstests_in_container.sh

RUN set -e; \
    chmod +x /usr/local/bin/slayerfs /usr/local/bin/xfstests_report.sh /usr/local/bin/run_xfstests_in_container.sh; \
    mkdir -p /opt/xfstests-dev /mnt/slayerfs /run/slayerfs /var/lib/slayerfs/data /artifacts; \
    tar -xzf /opt/xfstests-prebuilt.tar.gz -C /opt --transform 's|^xfstests|xfstests-dev|'; \
    chmod +x /opt/xfstests-dev/check /opt/xfstests-dev/src/* 2>/dev/null || true; \
    cp -f /opt/xfstests_slayer.exclude /opt/xfstests-dev/xfstests_slayer.exclude; \
    rm -f /opt/xfstests-prebuilt.tar.gz; \
    grep -q '^user_allow_other$' /etc/fuse.conf 2>/dev/null || echo 'user_allow_other' >> /etc/fuse.conf

ENV SLAYERFS_HOME=/var/lib/slayerfs \
    SLAYERFS_MOUNT_POINT=/mnt/slayerfs \
    SLAYERFS_DATA_BACKEND=local-fs \
    SLAYERFS_DATA_DIR=/var/lib/slayerfs/data \
    SLAYERFS_S3_BUCKET=slayerfs-data \
    SLAYERFS_S3_ENDPOINT=http://rustfs:9000 \
    SLAYERFS_S3_REGION=us-east-1 \
    SLAYERFS_S3_FORCE_PATH_STYLE=true \
    SLAYERFS_META_BACKEND=redis \
    SLAYERFS_META_URL=redis://redis:6379/0 \
    SLAYERFS_CONFIG_PATH=/run/slayerfs/config.yaml \
    SLAYERFS_LOG_FILE=/artifacts/slayerfs.log \
    XFSTESTS_DIR=/opt/xfstests-dev \
    RUST_LOG=slayerfs=info

VOLUME ["/mnt/slayerfs", "/var/lib/slayerfs", "/artifacts"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/run_xfstests_in_container.sh"]
