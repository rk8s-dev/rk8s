#!/usr/bin/env bats   -*- bats -*-
#
# basic netavark tests
#

load helpers


HELPER_PID=
function teardown() {
	if [[ -n "$HELPER_PID" ]]; then
		kill -9 $HELPER_PID
	fi
	basic_teardown
}

# custom DNS server is set to `127.0.0.255` which is invalid DNS server
# hence all the external request must fail, this test is expected to fail
# with exit code 124
@test "basic container - dns itself (custom bad dns server)" {
	setup_dnsmasq

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" custom_dns_server='"127.0.0.255"' aliases='"a1", "1a"'
	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "+short" "aone" "@$gw"
	assert "$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

    # custom dns server is set to 127.0.0.255 which is not a valid DNS server so external DNS request must fail
	expected_rc=124 run_in_container_netns "$a1_pid" "dig" "+short" "$TEST_DOMAIN" "@$gw"
}

# custom DNS server is set to `8.8.8.8, 1.1.1.1` which is valid DNS server
# hence all the external request must paas.
@test "basic container - dns itself (custom good dns server)" {
	setup_dnsmasq

	# launch dnsmasq to run a second local server with a unique name so we know custom_dns_server works
	run_in_host_netns dnsmasq --conf-file=/dev/null --pid-file="$AARDVARK_TMPDIR/dnsmasq2.pid" \
		--except-interface=lo --listen-address=127.1.1.53 --bind-interfaces  \
		--address=/unique-name.local/192.168.0.1 --no-resolv --no-hosts
	HELPER_PID=$(cat $AARDVARK_TMPDIR/dnsmasq2.pid)

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" custom_dns_server='"127.1.1.53"' aliases='"a1", "1a"'

	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "aone" "@$gw"
	# check for TTL 0 here as well
	assert "$output" =~ "aone\.[[:space:]]*0[[:space:]]*IN[[:space:]]*A[[:space:]]*$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

	run_in_container_netns "$a1_pid" "dig" "+short" "unique-name.local" "@$gw"
	# validate that we get the right ip
	assert "$output" == "192.168.0.1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"
}

@test "basic container - dns itself (bad and good should fall back)" {
	setup_dnsmasq

	# using exec to keep the udp query hanging for at least 3 seconds
	nsenter -m -n -t $HOST_NS_PID socat UDP4-LISTEN:53,bind=127.5.5.5 EXEC:"sleep 3" 3>/dev/null &
	HELPER_PID=$!

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" custom_dns_server='"127.5.5.5", "127.0.0.1"' aliases='"a1", "1a"'
	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID

    # first custom server is wrong but second server should work
	run_in_container_netns "$a1_pid" "dig" "$TEST_DOMAIN" "@$gw"
	assert "$output" =~ "Query time: [23][0-9]{3} msec" "timeout should be 2.5s so request should then work shortly after (udp)"

	kill -9 "$HELPER_PID" || true
	# Now the same with tcp.
	nsenter -m -n -t $HOST_NS_PID socat TCP4-LISTEN:53,bind=127.5.5.5 EXEC:"sleep 3" 3>/dev/null &
	HELPER_PID=$!
	run_in_container_netns "$a1_pid" "dig" +tcp "$TEST_DOMAIN" "@$gw"
	assert "$output" =~ "Query time: [23][0-9]{3} msec" "timeout should be 2.5s so request should then work shortly after (tcp)"
}

@test "basic container - dns itself custom" {
	setup_dnsmasq

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" aliases='"a1", "1a"'
	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "+short" "aone" "@$gw"
	assert "$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

	# check TCP support
	run_in_container_netns "$a1_pid" "dig" "+tcp" "+short" "aone" "@$gw"
	assert "$ip_a1"

	# check multiple TCP requests over single connecting by using +keepopen
	# https://github.com/containers/aardvark-dns/issues/605
	run_in_container_netns "$a1_pid" "dig" "+tcp" "+short" +keepopen "@$gw" "aone" "a1" "1a"
	assert "${lines[0]}" == "$ip_a1" "ip for aone"
	assert "${lines[1]}" == "$ip_a1" "ip for a1"
	assert "${lines[2]}" == "$ip_a1" "ip for 1a"

	run_in_container_netns "$a1_pid" "dig" "+short" "$TEST_DOMAIN" "@$gw"
	# validate that we get an ipv4
	assert "$output" =~ "[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

	# check TCP support for forwarding
	# note there is no guarantee that the forwarding is happening via TCP though
	# TODO add custom dns record that is to big for udp so we can be sure...
	run_in_container_netns "$a1_pid" "dig" "+tcp" "$TEST_DOMAIN" "@$gw"
	# validate that we get an ipv4
	assert "$output" =~ "[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+"
	# TODO This is not working on rhel/centos 9 as the dig version there doesn't print the line,
	# so we trust that dig +tcp does the right thing.
	# assert "$output" =~ "\(TCP\)" "server used TCP"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"
}

@test "basic container - ndots incomplete entry" {
	setup_dnsmasq

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" \
		subnet="$subnet_a" aliases='"a1", "1a"'
	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "someshortname" "@$gw"
	assert "$output" =~ "status: REFUSED" "dnsmasq returns REFUSED"

	run_in_container_netns "$a1_pid" "dig" "+short" "testname" "@$gw"
	assert "198.51.100.1" "should resolve local name from external nameserver (dnsmasq)"
}

@test "basic container - dns itself on container with ipaddress v6" {
	setup_dnsmasq

	subnet_a=$(random_subnet 6)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" aliases='"a1", "1a"'
	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "+short" "aone" "@$gw" "AAAA"
	assert "$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

	run_in_container_netns "$a1_pid" "dig" "+short" "$TEST_DOMAIN" "@$gw" "AAAA"
	# validate that we got valid ipv6
	# check that the output is not empty
	assert "$lines[0]" != "" "got at least one result"
	for ip in "${lines[@]}"; do
		run_helper ipcalc -6c "$ip"
	done
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"
}

@test "basic container - dns itself with long network name" {
	subnet_a=$(random_subnet 5)
	long_name="podman11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"
	create_config network_name="$long_name" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" aliases='"a1", "1a"'
	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.$long_name.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.$long_name.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "+short" "aone" "@$gw"
	assert "$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"
}

@test "two containers on the same network" {
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

	# Resolve container names to IPs
	dig "$a1_pid" "atwo" "$gw"
	assert "$a2_ip"
	# Set recursion bit
        assert "$output" !~ "WARNING: recursion requested but not available"
	dig "$a2_pid" "aone" "$gw"
	assert "$a1_ip"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"
}

# Internal network, meaning no DNS servers.
# Hence all external requests must fail.
@test "basic container - internal network has no DNS" {
	setup_dnsmasq

	subnet_a=$(random_subnet)
	create_config network_name="podman1" internal=true container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" custom_dns_server='"1.1.1.1","8.8.8.8"' aliases='"a1", "1a"'
	config_a1=$config
	# Network name is still recorded as podman1
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "+short" "aone" "@$gw"
	assert "$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

	# Internal network means no DNS server means this should hard-fail
	expected_rc=1 run_in_container_netns "$a1_pid" "host" "-t" "ns" "$TEST_DOMAIN" "$gw"
	assert "$output" =~ "Host $TEST_DOMAIN not found"
	assert "$output" =~ "NXDOMAIN"
}

# Internal network, but this time with IPv6. Same result as above expected.
@test "basic container - internal network has no DNS - ipv6" {
	setup_dnsmasq

	subnet_a=$(random_subnet 6)
	# Cloudflare and Google public anycast DNS v6 nameservers
	create_config network_name="podman1" internal=true container_id=$(random_string 64) container_name="aone" subnet="$subnet_a" custom_dns_server='"2606:4700:4700::1111","2001:4860:4860::8888"' aliases='"a1", "1a"'
	config_a1=$config
	# Network name is still recorded as podman1
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "+short" "aone" "@$gw" "AAAA"
	assert "$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

	# Internal network means no DNS server means this should hard-fail
	expected_rc=1 run_in_container_netns "$a1_pid" "host" "-t" "ns" "$TEST_DOMAIN" "$gw"
	assert "$output" =~ "Host $TEST_DOMAIN not found"
	assert "$output" =~ "NXDOMAIN"
}

@test "host dns on ipv6 link local" {
	# create a local interface with a link local ipv6 address
	# disable dad as it takes some time so the initial connection fails without it
	run_in_host_netns sysctl -w net.ipv6.conf.default.accept_dad=0
	run_in_host_netns ip link set lo up
	run_in_host_netns ip link add test type bridge
	run_in_host_netns ip link set test up
	run_in_host_netns ip -j addr
	link_local_addr=$(jq -r '.[] | select(.ifname=="test").addr_info[0].local' <<<"$output")

	# update our fake netns resolv.conf with the link local address as only nameserver
	echo "nameserver $link_local_addr%test" >"$AARDVARK_TMPDIR/resolv.conf"
	run_in_host_netns mount --bind "$AARDVARK_TMPDIR/resolv.conf" /etc/resolv.conf

	# launch dnsmasq to run a second local server with a unique name so we know custom_dns_server works
	run_in_host_netns dnsmasq --conf-file=/dev/null --pid-file="$AARDVARK_TMPDIR/dnsmasq2.pid" \
		--except-interface=lo --listen-address="$link_local_addr" --bind-interfaces  \
		--address=/unique-name.local/192.168.0.1 --no-resolv --no-hosts
	HELPER_PID=$(cat $AARDVARK_TMPDIR/dnsmasq2.pid)

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a"

	config_a1=$config
	ip_a1=$(echo "$config_a1" | jq -r .networks.podman1.static_ips[0])
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID
	run_in_container_netns "$a1_pid" "dig" "aone" "@$gw"
	# check for TTL 0 here as well
	assert "$output" =~ "aone\.[[:space:]]*0[[:space:]]*IN[[:space:]]*A[[:space:]]*$ip_a1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"

	run_in_container_netns "$a1_pid" "dig" "+short" "unique-name.local" "@$gw"
	# validate that we get the right ip
	assert "$output" == "192.168.0.1"
	# Set recursion bit is already set if requested so output must not
	# contain unexpected warning.
	assert "$output" !~ "WARNING: recursion requested but not available"
}

@test "nameservers updated when resolv.conf is modified" {
	setup_dnsmasq

	# Set up second dnsmasq server with different IP
	run_in_host_netns dnsmasq --conf-file=/dev/null --pid-file="$AARDVARK_TMPDIR/dnsmasq_second.pid" \
		--except-interface=lo --listen-address=127.1.1.2 --bind-interfaces \
		--address=/second-server.test/192.168.100.2 --no-resolv --no-hosts
	HELPER_PID=$(cat $AARDVARK_TMPDIR/dnsmasq_second.pid)

	subnet_a=$(random_subnet 5)
	create_config network_name="podman1" container_id=$(random_string 64) container_name="aone" subnet="$subnet_a"
	config_a1=$config
	gw=$(echo "$config_a1" | jq -r .network_info.podman1.subnets[0].gateway)
	create_container "$config_a1"
	a1_pid=$CONTAINER_NS_PID

	# Resolve using the first DNS server
	run_in_container_netns "$a1_pid" "dig" "+short" "testname" "@$gw"
	assert "$output" == "198.51.100.1" "should resolve using first DNS server"

	# Cannot resolve second server's domain yet
	expected_rc=1 run_in_container_netns "$a1_pid" "host" "-t" "a" "second-server.test" "$gw"
	assert "$output" =~ "not found" "should not resolve second server's domain initially"

	# Update resolv.conf to point to second DNS server
    echo "nameserver 127.1.1.2" > "$AARDVARK_TMPDIR/resolv.conf"

	retries=20
	while [[ $retries -gt 0 ]]; do
		expected_rc="?" run_in_container_netns "$a1_pid" "host" "-t" "a" "second-server.test" "$gw"
		if [[ $status -eq 0 ]]; then
			break
		fi
		sleep 0.5
		retries=$((retries -1))
	done

	# Resolve using the second DNS server
	run_in_container_netns "$a1_pid" "dig" "+short" "second-server.test" "@$gw"
	assert "$output" == "192.168.100.2" "should resolve using second DNS server after resolv.conf change"
}
