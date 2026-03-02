#!/usr/bin/env bats   -*- bats -*-
#
# basic netavark tests
#

load helpers

@test "two containers on the same network with aliases" {
	# container a1
	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" aliases='"a1", "1a"'
	config_a1="$config"
	a1_ip=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID

	# container a2
	create_config network_name="podman1" container_id=$(random_string 64) container_name="atwo" subnet="$subnet_a" aliases='"a2", "2a"'
	config_a2="$config"
	a2_ip=$(echo "$config_a2" | jq -r .networks.podman1.static_ips[0])
	create_container "$config_a2"
	a2_pid="$CONTAINER_NS_PID"

	dig "$a1_pid" "a2" "$gw"
	assert "$a2_ip"
	dig "$a1_pid" "2a" "$gw"
	assert "$a2_ip"
	dig "$a2_pid" "a1" "$gw"
	assert "$a1_ip"
	dig "$a2_pid" "1a" "$gw"
	assert "$a1_ip"
}
