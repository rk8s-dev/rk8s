#!/usr/bin/env bats   -*- bats -*-
#
# basic netavark tests
#

load helpers


SOCAT_PID=

function teardown() {
    kill -9 $SOCAT_PID
    basic_teardown
}

# check bind error on startup
@test "aardvark-dns should fail when udp port is already bound" {
	# bind the port to force a failure for aardvark-dns
	# we cannot use run_is_host_netns to run in the background
	nsenter -m -n -t $HOST_NS_PID socat UDP4-LISTEN:53 - 3> /dev/null &
	SOCAT_PID=$!

	# ensure socat has time to bind the port
	sleep 1

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a"
    gw=$(echo "$config" | jq -r .network_info.podman1.subnets[0].gateway)
	expected_rc=1 create_container "$config"
    assert "$output" =~ "failed to bind udp listener on $gw:53" "bind error message"
}

@test "aardvark-dns should fail when tcp port is already bound" {
	# bind the port to force a failure for aardvark-dns
	# we cannot use run_is_host_netns to run in the background
	nsenter -m -n -t $HOST_NS_PID socat TCP4-LISTEN:53 - 3> /dev/null &
	SOCAT_PID=$!

	# ensure socat has time to bind the port
	sleep 1

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a"
    gw=$(echo "$config" | jq -r .network_info.podman1.subnets[0].gateway)
	expected_rc=1 create_container "$config"
    assert "$output" =~ "failed to bind tcp listener on $gw:53" "bind error message"
}
