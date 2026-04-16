FROM quay.io/coreos/etcd:v3.6.0 AS etcdctl-bin

FROM alpine:3.20

RUN apk add --no-cache ca-certificates

COPY --from=etcdctl-bin /usr/local/bin/etcdctl /usr/local/bin/etcdctl
COPY etcd-maintenance.sh /usr/local/bin/etcd-maintenance.sh

RUN chmod +x /usr/local/bin/etcdctl /usr/local/bin/etcd-maintenance.sh

ENTRYPOINT ["/bin/sh", "/usr/local/bin/etcd-maintenance.sh"]