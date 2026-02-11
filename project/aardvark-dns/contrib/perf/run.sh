#!/bin/bash

PODMAN=${PODMAN-podman}
DIR=$(dirname -- "${BASH_SOURCE[0]}")
IMAGE=docker.io/library/python
JOBS=${JOBS:-$(nproc)}
netname="testnet"


$PODMAN rm -fa -t0
$PODMAN network rm -f $netname

$PODMAN network create $netname

# first command to spawn aardvark-dns
$PODMAN run -i -d --network $netname --name starter $IMAGE

perf stat -p $(pgrep -n aardvark-dns) &> $DIR/perf.log &

for i in $( seq 1 $JOBS )
do
    $PODMAN run -v $DIR/nslookup.py:/nslookup.py:z --name test$i --network $netname:alias=testabc$i -d $IMAGE /nslookup.py testabc$i
done

$PODMAN rm -f -t0 starter

# wait for perf to finish
# because aardvark-dns exists on its own when all containers are done this should not hang
wait

#
$PODMAN rm -fa -t0
$PODMAN network rm -f $netname
