#!/usr/bin/env bats   -*- bats -*-
#
# basic netavark tests
#

load helpers

@test "check reverse lookups" {
	# container a1
	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" aliases='"a1", "1a"'
	a1_config="$config"
	a1_ip=$(echo "$a1_config" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$a1_config" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$a1_config"
	a1_pid=$CONTAINER_NS_PID

	# container a2
	create_config network_name="podman1" container_id=$(random_string 64) container_name="atwo" subnet="$subnet_a" aliases='"a2", "2a"'
	a2_config="$config"
	a2_ip=$(echo "$a2_config" | jq -r .networks.podman1.static_ips[0])
	create_container "$a2_config"
	a2_pid="$CONTAINER_NS_PID"

	echo "a1 config:\n${a1_config}\n"
	echo "a2 config:\n${a2_config}\n"

	# Resolve IPs to container names
	dig_reverse "$a1_pid" "$a2_ip" "$gw"
	echo -e "Output:\n${output}\n"
	a2_expected_name=$(echo $a2_ip | awk -F. '{printf "%d.%d.%d.%d.in-addr.arpa.", $4, $3, $2, $1}')
	assert "$output" =~ "$a2_expected_name[[:space:]]*0[[:space:]]*IN[[:space:]]*PTR[[:space:]]*atwo\."
	assert "$output" =~ "$a2_expected_name[[:space:]]*0[[:space:]]*IN[[:space:]]*PTR[[:space:]]*a2\."
	assert "$output" =~ "$a2_expected_name[[:space:]]*0[[:space:]]*IN[[:space:]]*PTR[[:space:]]*2a\."
	dig_reverse "$a2_pid" "$a1_ip" "$gw"
	echo -e "Output:\n${output}\n"
	a1_expected_name=$(echo $a1_ip | awk -F. '{printf "%d.%d.%d.%d.in-addr.arpa.", $4, $3, $2, $1}')
	assert "$output" =~ "$a1_expected_name[[:space:]]*0[[:space:]]*IN[[:space:]]*PTR[[:space:]]*aone\."
	assert "$output" =~ "$a1_expected_name[[:space:]]*0[[:space:]]*IN[[:space:]]*PTR[[:space:]]*a1\."
	assert "$output" =~ "$a1_expected_name[[:space:]]*0[[:space:]]*IN[[:space:]]*PTR[[:space:]]*1a\."
}

@test "check reverse lookups on ipaddress v6" {
	# container a1
	subnet_a=$(random_subnet 6)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" aliases='"a1", "1a"'
	a1_config="$config"
	a1_ip=$(echo "$a1_config" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$a1_config" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$a1_config"
	a1_pid=$CONTAINER_NS_PID

	# container a2
	create_config network_name="podman1" container_id=$(random_string 64) container_name="atwo" subnet="$subnet_a" aliases='"a2", "2a"'
	a2_config="$config"
	a2_ip=$(echo "$a2_config" | jq -r .networks.podman1.static_ips[0])
	create_container "$a2_config"
	a2_pid="$CONTAINER_NS_PID"

	echo "$a1_config"
	echo "$a2_config"

	# Resolve IPs to container names
	# It is much harder to construct the arpa address in ipv6 so we just check that we are in the fd::/8 range
	dig_reverse "$a1_pid" "$a2_ip" "$gw"
	assert "$output" =~ '([0-9a-f]\.){30}d\.f\.ip6\.arpa\.[ 	].*[ 	]atwo\.'
	assert "$output" =~ '([0-9a-f]\.){30}d\.f\.ip6\.arpa\.[ 	].*[ 	]a2\.'
	assert "$output" =~ '([0-9a-f]\.){30}d\.f\.ip6\.arpa\.[ 	].*[ 	]2a\.'
	dig_reverse "$a2_pid" "$a1_ip" "$gw"
	assert "$output" =~ '([0-9a-f]\.){30}d\.f\.ip6\.arpa\.[ 	].*[ 	]aone\.'
	assert "$output" =~ '([0-9a-f]\.){30}d\.f\.ip6\.arpa\.[ 	].*[ 	]a1\.'
	assert "$output" =~ '([0-9a-f]\.){30}d\.f\.ip6\.arpa\.[ 	].*[ 	]1a\.'
}
