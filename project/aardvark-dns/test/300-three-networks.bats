#!/usr/bin/env bats   -*- bats -*-
#
# basic netavark tests
#

load helpers

@test "three networks with a connect" {
	setup_dnsmasq

	subnet_a=$(random_subnet 5)
	subnet_b=$(random_subnet 5)

	# A1
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a"
	a1_config=$config
	a1_container_id=$(echo "$a1_config" | jq -r .container_id)
	a1_ip=$(echo "$a1_config" | jq -r .networks.podman1.static_ips[0])
	a_gw=$(echo "$a1_config" | jq -r .network_info.podman1.subnets[0].gateway)
	a1_hash=$(echo "$a1_config" | jq -r .network_info.podman1.id)
	create_container "$a1_config"
	a1_pid=$CONTAINER_NS_PID

	# container b1 on subnet b
	create_config network_name="podman2" container_id=$(random_string 64) container_name="bone" subnet="$subnet_b"
	b1_config=$config
	b1_ip=$(echo "$b1_config" | jq -r .networks.podman2.static_ips[0])
	b_gw=$(echo "$b1_config" | jq -r .network_info.podman2.subnets[0].gateway)
	b1_hash=$(echo "$b1_config" | jq -r .network_info.podman1.id)
	create_container "$b1_config"
	b1_pid=$CONTAINER_NS_PID
	b_subnets=$(echo $b1_config | jq -r .network_info.podman2.subnets[0])

	# AB2
	create_config network_name="podman1" container_id=$(random_string 64) container_name="abtwo" subnet="$subnet_a"
	a2_config=$config
	a2_ip=$(echo "$a2_config" | jq -r .networks.podman1.static_ips[0])

	b2_ip=$(random_ip_in_subnet "$subnet_b")
	create_network "podman2" "$b2_ip" "eth1"
	b2_network="{$new_network}"
	create_network_infos "podman2" "$b1_hash" "$b_subnets"
	b2_network_info="{$new_network_info}"
	ab2_config=$(jq -r ".networks +=  $b2_network" <<<"$a2_config")
	ab2_config=$(jq -r ".network_info += $b2_network_info" <<<"$ab2_config")

	create_container "$ab2_config"
	ab2_pid=$CONTAINER_NS_PID

	# aone should be able to resolve AB2 and NOT B1
	dig "$a1_pid" "abtwo" "$a_gw"
	assert "$a2_ip"
	dig "$a1_pid" "bone" "$a_gw"
	assert ""

	# bone should be able to resolve AB2 and NOT A1
	dig "$b1_pid" "abtwo" "$b_gw"
	assert "$b2_ip"
	dig "$b1_pid" "aone" "$b_gw"
	assert ""

	# abtwo should be able to resolve A1, B1, and AB2 on both gws
	dig "$ab2_pid" "aone" "$a_gw"
	assert "$a1_ip"
	dig "$ab2_pid" "aone" "$b_gw"
	assert "$a1_ip"

	dig "$ab2_pid" "bone" "$a_gw"
	assert "$b1_ip"
	dig "$ab2_pid" "bone" "$b_gw"
	assert "$b1_ip"

	# now the same again with search domain set
	dig "$ab2_pid" "aone.dns.podman" "$a_gw"
	assert "$a1_ip"
	dig "$ab2_pid" "aone.dns.podman" "$b_gw"
	assert "$a1_ip"

	dig "$ab2_pid" "bone.dns.podman" "$a_gw"
	assert "$b1_ip"
	dig "$ab2_pid" "bone.dns.podman" "$b_gw"
	assert "$b1_ip"

	# check ab2 from itself, first from the a side
	dig "$ab2_pid" "abtwo" "$a_gw"
	assert "${#lines[@]}" = 2
	assert "$output" =~ "$a2_ip"
	assert "$output" =~ "$b2_ip"

	# and now from the bside
	dig "$ab2_pid" "abtwo" "$b_gw"
	assert "${#lines[@]}" = 2
	assert "$output" =~ "$a2_ip"
	assert "$output" =~ "$b2_ip"
}

@test "three subnets, one container on two of the subnets, network connect" {
	# Create all three subnets
	subnet_a=$(random_subnet 5)
	subnet_b=$(random_subnet 5)
	subnet_c=$(random_subnet 5)

	# A1 on subnet A
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a"
	a1_config=$config
	a1_container_id=$(echo "$a1_config" | jq -r .container_id)
	a1_ip=$(echo "$a1_config" | jq -r .networks.podman1.static_ips[0])
	a_gw=$(echo "$a1_config" | jq -r .network_info.podman1.subnets[0].gateway)
	a1_hash=$(echo "$a1_config" | jq -r .network_info.podman1.id)
	create_container "$a1_config"
	a1_pid=$CONTAINER_NS_PID

	# C1 on subnet C
	create_config network_name="podman3" container_id=$(random_string 64) container_name="cone" subnet="$subnet_c"
	c1_config=$config
	c1_container_id=$(echo "$c1_config" | jq -r .container_id)
	c1_ip=$(echo "$c1_config" | jq -r .networks.podman3.static_ips[0])
	c_gw=$(echo "$c1_config" | jq -r .network_info.podman3.subnets[0].gateway)
	c1_hash=$(echo "$c1_config" | jq -r .network_info.podman3.id)
	create_container "$c1_config"
	c1_pid=$CONTAINER_NS_PID
	c_subnets=$(echo $c1_config | jq -r .network_info.podman3.subnets[0])

	# We now have one container on A and one on C.  We now similate
	# a network connect on both to B.
	#
	# This is also where things get tricky and we are trying to mimic
	# a connect. First, we need to trim off the last two container
	# configs for teardown. We will leave the NS_PIDS alone because
	# the order should be OK.

	# Create B1 config for network connect
	create_config network_name="podman2" container_id=$(random_string 64) container_name="aone" subnet="$subnet_b" aliases='"aone_nw"'
	b1_config=$config
	# The container ID should be the same
	b1_config=$(jq ".container_id  |= \"$a1_container_id\"" <<<"$b1_config")
	b1_config=$(jq ".networks.podman2.interface_name |= \"eth1\"" <<<"$b1_config")
	b1_network=$(echo "$b1_config" | jq -r .networks)
	b1_network_info=$(echo "$b1_config" | jq -r .network_info)
	b1_ip=$(echo "$b1_network" | jq -r .podman2.static_ips[0])
	b_gw=$(echo "$b1_network_info" | jq -r .podman2.subnets[0].gateway)

	# Now we must merge a1 and b1 for eventual teardown
	a1b1_config=$(jq -r ".networks += $b1_network" <<<"$a1_config")
	a1b1_config=$(jq -r ".network_info += $b1_network_info" <<<"$a1b1_config")

	# Create B2 config for network connect
	#
	create_config network_name="podman2" container_id=$(random_string 64) container_name="cone" subnet="$subnet_b" aliases='"cone_nw"'
	b2_config=$config
	# The container ID should be the same
	b2_config=$(jq ".container_id  |= \"$c1_container_id\"" <<<"$b2_config")
	b2_config=$(jq ".networks.podman2.interface_name |= \"eth1\"" <<<"$b2_config")
	b2_network=$(echo "$b2_config" | jq -r .networks)
	b2_network_info=$(echo "$b2_config" | jq -r .network_info)
	b2_ip=$(echo "$b2_network" | jq -r .podman2.static_ips[0])

	# Now we must merge c1 and b2 for eventual teardown
	c1b2_config=$(jq -r ".networks += $b2_network" <<<"$c1_config")
	c1b2_config=$(jq -r ".network_info += $b2_network_info" <<<"$c1b2_config")

	# Create the containers but do not add to NS_PIDS or CONTAINER_CONFIGS
	connect "$a1_pid" "$b1_config"
	connect "$c1_pid" "$b2_config"

	# Reset CONTAINER_CONFIGS and add the two news ones
	CONTAINER_CONFIGS=("$a1b1_config" "$c1b2_config")

	# Verify
	# b1 should be able to resolve cone through b subnet
	dig "$a1_pid" "cone" "$b_gw"
	assert "$b2_ip"

	# a1 should be able to resolve cone
	dig "$a1_pid" "cone" "$a_gw"
	assert "$b2_ip"

	# a1b1 should be able to resolve cone_nw alias
	dig "$a1_pid" "cone_nw" "$a_gw"
	assert "$b2_ip"

	# b2 should be able to resolve cone through b subnet
	dig "$c1_pid" "aone" "$b_gw"
	assert "$b1_ip"

	# c1 should be able to resolve aone
	dig "$c1_pid" "aone" "$c_gw"
	assert "$b1_ip"

	# b2c1 should be able to resolve aone_nw alias
	dig "$c1_pid" "aone_nw" "$c_gw"
	assert "$b1_ip"
}


@test "three subnets two ipaddress v6 and one ipaddress v4, one container on two of the subnets, network connect" {
	# Create all three subnets
	# Two of the subnets must be on ip addresss v6 and one on ip address v4
	subnet_a=$(random_subnet 5)
	subnet_b=$(random_subnet 6)
	subnet_c=$(random_subnet 6)

	# A1 on subnet A
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a"
	a1_config=$config
	a1_container_id=$(echo "$a1_config" | jq -r .container_id)
	a1_ip=$(echo "$a1_config" | jq -r .networks.podman1.static_ips[0])
	a_gw=$(echo "$a1_config" | jq -r .network_info.podman1.subnets[0].gateway)
	a1_hash=$(echo "$a1_config" | jq -r .network_info.podman1.id)
	create_container "$a1_config"
	a1_pid=$CONTAINER_NS_PID

	# C1 on subnet C
	create_config network_name="podman3" container_id=$(random_string 64) container_name="cone" subnet="$subnet_c"
	c1_config=$config
	c1_container_id=$(echo "$c1_config" | jq -r .container_id)
	c1_ip=$(echo "$c1_config" | jq -r .networks.podman3.static_ips[0])
	c_gw=$(echo "$c1_config" | jq -r .network_info.podman3.subnets[0].gateway)
	c1_hash=$(echo "$c1_config" | jq -r .network_info.podman3.id)
	create_container "$c1_config"
	c1_pid=$CONTAINER_NS_PID
	c_subnets=$(echo $c1_config | jq -r .network_info.podman3.subnets[0])

	# We now have one container on A and one on C.  We now similate
	# a network connect on both to B.

	# Create B1 config for network connect
	create_config network_name="podman2" container_id=$(random_string 64) container_name="aone" subnet="$subnet_b" aliases='"aone_nw"'
	b1_config=$config
	# The container ID should be the same
	b1_config=$(jq ".container_id  |= \"$a1_container_id\"" <<<"$b1_config")
	b1_config=$(jq ".networks.podman2.interface_name |= \"eth1\"" <<<"$b1_config")
	b1_network=$(echo "$b1_config" | jq -r .networks)
	b1_network_info=$(echo "$b1_config" | jq -r .network_info)
	b1_ip=$(echo "$b1_network" | jq -r .podman2.static_ips[0])
	b_gw=$(echo "$b1_network_info" | jq -r .podman2.subnets[0].gateway)

	# Now we must merge a1 and b1 for eventual teardown
	a1b1_config=$(jq -r ".networks += $b1_network" <<<"$a1_config")
	a1b1_config=$(jq -r ".network_info += $b1_network_info" <<<"$a1b1_config")

	# Create B2 config for network connect
	#
	create_config network_name="podman2" container_id=$(random_string 64) container_name="cone" subnet="$subnet_b" aliases='"cone_nw"'
	b2_config=$config
	# The container ID should be the same
	b2_config=$(jq ".container_id  |= \"$c1_container_id\"" <<<"$b2_config")
	b2_config=$(jq ".networks.podman2.interface_name |= \"eth1\"" <<<"$b2_config")
	b2_network=$(echo "$b2_config" | jq -r .networks)
	b2_network_info=$(echo "$b2_config" | jq -r .network_info)
	b2_ip=$(echo "$b2_network" | jq -r .podman2.static_ips[0])

	# Now we must merge c1 and b2 for eventual teardown
	c1b2_config=$(jq -r ".networks += $b2_network" <<<"$c1_config")
	c1b2_config=$(jq -r ".network_info += $b2_network_info" <<<"$c1b2_config")

	# Create the containers but do not add to NS_PIDS or CONTAINER_CONFIGS
	connect "$a1_pid" "$b1_config"
	connect "$c1_pid" "$b2_config"

	# Reset CONTAINER_CONFIGS and add the two news ones
	CONTAINER_CONFIGS=("$a1b1_config" "$c1b2_config")

	# Verify
	# b1 should be able to resolve cone through b subnet
	dig "$a1_pid" "cone" "$b_gw" "AAAA"
	assert "$b2_ip"

	# a1 should be able to resolve cone
	dig "$a1_pid" "cone" "$a_gw" "AAAA"
	assert "$b2_ip"
}
