# -*- bash -*-

# Netavark binary to run
NETAVARK=${NETAVARK:-/usr/libexec/podman/netavark}

TESTSDIR=${TESTSDIR:-$(dirname ${BASH_SOURCE})}

AARDVARK=${AARDVARK:-$TESTSDIR/../bin/aardvark-dns}

# export RUST_BACKTRACE so that we get a helpful stack trace
export RUST_BACKTRACE=full

TEST_DOMAIN=example.podman.io

HOST_NS_PID=
CONTAINER_NS_PID=

CONTAINER_CONFIGS=()
CONTAINER_NS_PIDS=()

#### Functions below are taken from podman and buildah and adapted to netavark.

################
#  run_helper  #  Invoke args, with timeout, using BATS 'run'
################
#
# Second, we use 'timeout' to abort (with a diagnostic) if something
# takes too long; this is preferable to a CI hang.
#
# Third, we log the command run and its output. This doesn't normally
# appear in BATS output, but it will if there's an error.
#
# Next, we check exit status. Since the normal desired code is 0,
# that's the default; but the expected_rc var can override:
#
#     expected_rc=125 run_helper nonexistent-subcommand
#     expected_rc=?   run_helper some-other-command       # let our caller check status
#
# Since we use the BATS 'run' mechanism, $output and $status will be
# defined for our caller.
#
function run_helper() {
    # expected_rc if unset set default to 0
    expected_rc="${expected_rc-0}"
    if [ "$expected_rc" == "?" ]; then
        expected_rc=
    fi
    # Remember command args, for possible use in later diagnostic messages
    MOST_RECENT_COMMAND="$*"

    # stdout is only emitted upon error; this echo is to help a debugger
    echo "$_LOG_PROMPT $*"

    # BATS hangs if a subprocess remains and keeps FD 3 open; this happens
    # if a process crashes unexpectedly without cleaning up subprocesses.
    run timeout --foreground -v --kill=10 10 "$@" 3>&-
    # without "quotes", multiple lines are glommed together into one
    if [ -n "$output" ]; then
        echo "$output"
    fi
    if [ "$status" -ne 0 ]; then
        echo -n "[ rc=$status "
        if [ -n "$expected_rc" ]; then
            if [ "$status" -eq "$expected_rc" ]; then
                echo -n "(expected) "
            else
                echo -n "(** EXPECTED $expected_rc **) "
            fi
        fi
        echo "]"
    fi

    if [ "$status" -eq 124 ]; then
        if expr "$output" : ".*timeout: sending" >/dev/null; then
            # It's possible for a subtest to _want_ a timeout
            if [[ "$expected_rc" != "124" ]]; then
                echo "*** TIMED OUT ***"
                false
            fi
        fi
    fi

    if [ -n "$expected_rc" ]; then
        if [ "$status" -ne "$expected_rc" ]; then
            die "exit code is $status; expected $expected_rc"
        fi
    fi

    # unset
    unset expected_rc
}

#########
#  die  #  Abort with helpful message
#########
function die() {
    # FIXME: handle multi-line output
    echo "#/vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv" >&2
    echo "#| FAIL: $*" >&2
    echo "#\\^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^" >&2
    false
}
############
#  assert  #  Compare actual vs expected string; fail if mismatch
############
#
# Compares string (default: $output) against the given string argument.
# By default we do an exact-match comparison against $output, but there
# are two different ways to invoke us, each with an optional description:
#
#      xpect               "EXPECT" [DESCRIPTION]
#      xpect "RESULT" "OP" "EXPECT" [DESCRIPTION]
#
# The first form (one or two arguments) does an exact-match comparison
# of "$output" against "EXPECT". The second (three or four args) compares
# the first parameter against EXPECT, using the given OPerator. If present,
# DESCRIPTION will be displayed on test failure.
#
# Examples:
#
#   xpect "this is exactly what we expect"
#   xpect "${lines[0]}" =~ "^abc"  "first line begins with abc"
#
function assert() {
    local actual_string="$output"
    local operator='=='
    local expect_string="$1"
    local testname="$2"

    case "${#*}" in
    0) die "Internal error: 'assert' requires one or more arguments" ;;
    1 | 2) ;;
    3 | 4)
        actual_string="$1"
        operator="$2"
        expect_string="$3"
        testname="$4"
        ;;
    *) die "Internal error: too many arguments to 'assert'" ;;
    esac

    # Comparisons.
    # Special case: there is no !~ operator, so fake it via '! x =~ y'
    local not=
    local actual_op="$operator"
    if [[ $operator == '!~' ]]; then
        not='!'
        actual_op='=~'
    fi
    if [[ $operator == '=' || $operator == '==' ]]; then
        # Special case: we can't use '=' or '==' inside [[ ... ]] because
        # the right-hand side is treated as a pattern... and '[xy]' will
        # not compare literally. There seems to be no way to turn that off.
        if [ "$actual_string" = "$expect_string" ]; then
            return
        fi
    elif [[ $operator == '!=' ]]; then
        # Same special case as above
        if [ "$actual_string" != "$expect_string" ]; then
            return
        fi
    else
        if eval "[[ $not \$actual_string $actual_op \$expect_string ]]"; then
            return
        elif [ $? -gt 1 ]; then
            die "Internal error: could not process 'actual' $operator 'expect'"
        fi
    fi

    # Test has failed. Get a descriptive test name.
    if [ -z "$testname" ]; then
        testname="${MOST_RECENT_BUILDAH_COMMAND:-[no test name given]}"
    fi

    # Display optimization: the typical case for 'expect' is an
    # exact match ('='), but there are also '=~' or '!~' or '-ge'
    # and the like. Omit the '=' but show the others; and always
    # align subsequent output lines for ease of comparison.
    local op=''
    local ws=''
    if [ "$operator" != '==' ]; then
        op="$operator "
        ws=$(printf "%*s" ${#op} "")
    fi

    # This is a multi-line message, which may in turn contain multi-line
    # output, so let's format it ourself, readably
    local actual_split
    IFS=$'\n' read -rd '' -a actual_split <<<"$actual_string" || true
    printf "#/vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv\n" >&2
    printf "#|     FAIL: %s\n" "$testname" >&2
    printf "#| expected: %s'%s'\n" "$op" "$expect_string" >&2
    printf "#|   actual: %s'%s'\n" "$ws" "${actual_split[0]}" >&2
    local line
    for line in "${actual_split[@]:1}"; do
        printf "#|         > %s'%s'\n" "$ws" "$line" >&2
    done
    printf "#\\^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^\n" >&2
    false
}

#################
#  assert_json  #  Compare actual json vs expected string; fail if mismatch
#################
# assert_json works like assert except that it accepts one extra parameter,
# the jq query string.
# There are two different ways to invoke us, each with an optional description:
#
#      xpect               "JQ_QUERY"      "EXPECT" [DESCRIPTION]
#      xpect "JSON_STRING" "JQ_QUERY" "OP" "EXPECT" [DESCRIPTION]
# Important this function will overwrite $output, so if you need to use the value
# more than once you need to safe it in another variable.
function assert_json() {
    local actual_json="$output"
    local operator='=='
    local jq_query="$1"
    local expect_string="$2"
    local testname="$3"

    case "${#*}" in
    0 | 1) die "Internal error: 'assert_json' requires two or more arguments" ;;
    2 | 3) ;;
    4 | 5)
        actual_json="$1"
        jq_query="$2"
        operator="$3"
        expect_string="$4"
        testname="$5"
        ;;
    *) die "Internal error: too many arguments to 'assert_json'" ;;
    esac
    run_helper jq -r "$jq_query" <<<"$actual_json"
    assert "$output" "$operator" "$expect_string" "$testname"
}

###################
#  random_string  #  Pseudorandom alphanumeric string of given length
###################
function random_string() {
    local length=${1:-10}
    head /dev/urandom | tr -dc a-zA-Z0-9 | head -c$length
}

###################
#  random_subnet  # generate a random private subnet
###################
#
# by default it will return a 10.x.x.0/24 ipv4 subnet
# if "6" is given as first argument it will return a "fdx:x:x:x::/64" ipv6 subnet
function random_subnet() {
    if [[ "$1" == "6" ]]; then
        printf "fd%02x:%x:%x:%x::/64" $((RANDOM % 256)) $((RANDOM % 65535)) $((RANDOM % 65535)) $((RANDOM % 65535))
    else
        printf "10.%d.%d.0/24" $((RANDOM % 256)) $((RANDOM % 256))
    fi
}

#########################
#  random_ip_in_subnet  # get a random from a given subnet
#########################
# the first arg must be an subnet created by random_subnet
# otherwise this function might return an invalid ip
function random_ip_in_subnet() {
    # first trim subnet
    local net_ip=${1%/*}
    local num=
    local add=$2
    # if ip has colon it is ipv6
    if [[ "$net_ip" == *":"* ]]; then
        num=$((RANDOM % 65533 ))
        # see below
        num=$((num - num % 10 + add + 2))
        num=$(printf "%x" $num)
    else
        # if ipv4 we have to trim the final 0
        net_ip=${net_ip%0}
        # make sure to not get 0, 1 or 255
        num=$((RANDOM % 252))
        # Avoid giving out duplicated ips if we are called more than once.
        # The caller needs to keep a counter because this is executed ina subshell so we cannot use global var here.
        # Basically subtract mod 10 then add the counter so we can never get a dup ip assuming counter < 10 which
        # should always be the case here. Add 2 to avoid using .0 .1 which have special meaning.
        num=$((num - num % 10 + add + 2))
    fi
    printf "$net_ip%s" $num
}

#########################
#  gateway_from_subnet  # get the first ip from a given subnet
#########################
# the first arg must be an subnet created by random_subnet
# otherwise this function might return an invalid ip
function gateway_from_subnet() {
    # first trim subnet
    local net_ip=${1%/*}
    # set first ip in network as gateway
    local num=1
    # if ip has dor it is ipv4
    if [[ "$net_ip" == *"."* ]]; then
        # if ipv4 we have to trim the final 0
        net_ip=${net_ip%0}
    fi
    printf "$net_ip%s" $num
}

function create_netns() {
    # create a new netns and mountns and run a sleep process to keep it alive
    # we have to redirect stdout/err to /dev/null otherwise bats will hang
    unshare -mn sleep inf &>/dev/null &
    pid=$!

    # we have to wait for unshare and check that we have a new ns before returning
    local timeout=2
    while [[ $timeout -gt 0 ]]; do
        if [ "$(readlink /proc/self/ns/net)" != "$(readlink /proc/$pid/ns/net)" ]; then
            echo $pid
            return
        fi
        sleep 1
        let timeout=$timeout-1
    done

    die "Timed out waiting for unshare new netns"
}

function get_container_netns_path() {
    echo /proc/$1/ns/net
}

################
#  run_netavark  #  Invoke $NETAVARK, with timeout, using BATS 'run'
################
#
# This is the preferred mechanism for invoking netavark: first, it
# it joins the test network namespace before it invokes $NETAVARK,
# which may be 'netavark' or '/some/path/netavark'.
function run_netavark() {
    run_in_host_netns $NETAVARK "--config" "$AARDVARK_TMPDIR" "-a" "$AARDVARK" "$@"
}

################
#  run_in_container_netns  #  Run args in container netns
################
#
# first arg must be the container pid
function run_in_container_netns() {
    con_pid=$1
    shift
    run_helper nsenter -n -t $con_pid "$@"
}

################
#  run_in_host_netns  #  Run args in host netns
################
#
function run_in_host_netns() {
    run_helper nsenter -m -n -t $HOST_NS_PID "$@"
}

################
#  create_config#  Creates a config netavark can use
################
#
# The following arguments are supported, the order does not matter:
#     network_name=$network_name
#     container_id=$container_id
#     container_name=$container_name
#     subnet=$subnet specifies the network subnet
#     custom_dns_serve=$custom_dns_server
#     aliases=$aliases comma seperated container aliases for dns resolution.
#     internal={true,false} default is false
function create_config() {
    local network_name=""
    local container_id=""
    local container_name=""
    local subnet=""
    local custom_dns_server
    local aliases=""
    local internal=false

     # parse arguments
    while [[ "$#" -gt 0 ]]; do
        IFS='=' read -r arg value <<<"$1"
        case "$arg" in
        network_name)
            network_name="$value"
            ;;
        container_id)
            container_id="$value"
            ;;
        container_name)
            container_name="$value"
            ;;
        subnet)
            subnet="$value"
            ;;
        custom_dns_server)
            custom_dns_server="$value"
            ;;
        aliases)
            aliases="$value"
            ;;
        internal)
            internal="$value"
            ;;
        *) die "unknown argument for '$arg' create_config" ;;
        esac
        shift
    done

    container_ip=$(random_ip_in_subnet $subnet $IP_COUNT)
    IP_COUNT=$((IP_COUNT + 1))
    container_gw=$(gateway_from_subnet $subnet)
    subnets="{\"subnet\":\"$subnet\",\"gateway\":\"$container_gw\"}"

    create_network "$network_name" "$container_ip" "eth0" "$aliases"
    create_network_infos "$network_name" $(random_string 64) "$subnets" "$internal"

    read -r -d '\0' config <<EOF
{
  "container_id": "$container_id",
  "container_name": "$container_name",
  "networks": {
      $new_network
  },
  "network_info": {
      $new_network_info
  },
  "dns_servers": [$custom_dns_server]
}\0
EOF

}

################
#  create_network infos#  Creates a network_info json blob for netavark
################
# arg1 is network name
# arg2 network_id
# arg3 is subnets
# arg4 is internal
function create_network_infos() {
    local net_name=$1
    local net_id=$2
    local subnets=$3
    local internal=${4:-false}
    local interface_name=${net_name:0:7}

    read -r -d '\0' new_network_info <<EOF
    "$net_name": {
      "name": "$net_name",
      "id": "$net_id",
      "driver": "bridge",
      "network_interface": "$interface_name",
      "subnets": [
        $subnets
      ],
      "ipv6_enabled": true,
      "internal": $internal,
      "dns_enabled": true,
      "ipam_options": {
        "driver": "host-local"
      }
    }\0
EOF

}

################
#  create_network#  Creates a network json blob for netavark
################
# arg is network name
# arg is ip address
# arg is interface (ethX)
# arg are aliases
function create_network() {
    local net_name=$1
    shift
    local ip_address=$1
    shift
    local interface_name=$1
    shift
    local aliases=$1

    read -r -d '\0' new_network <<EOF
    "$net_name": {
      "static_ips": [
        "$ip_address"
	],
	  "aliases": [
		$aliases
	],
      "interface_name": "$interface_name"
    }\0
EOF

}

################
#  create container#  Creates a netns that mimics a container
################
# arg1 is config
function create_container() {
    CONTAINER_NS_PID=$(create_netns)
    CONTAINER_NS_PIDS+=("$CONTAINER_NS_PID")
    CONTAINER_CONFIGS+=("$1")
    create_container_backend "$CONTAINER_NS_PID" "$1"
}

# arg1 is pid
# arg2 is config
function create_container_backend() {
    run_netavark setup $(get_container_netns_path $1) <<<"$2"
}

################
#  connect#  Connects netns to another network
################
# arg1 is pid
# arg2 is config
function connect() {
    create_container_backend "$1" "$2"
}

function basic_host_setup() {
    HOST_NS_PID=$(create_netns)
    # make sure to set DBUS_SYSTEM_BUS_ADDRESS to an empty value
    # netavark will try to use firewalld connection when possible
    # because we run in a separate netns we cannot use firewalld
    # firewalld run in the host netns and not our custom netns
    # thus the firewall rules end up in the wrong netns
    # unsetting does not work, it would use the default address
    export DBUS_SYSTEM_BUS_ADDRESS=
    AARDVARK_TMPDIR=$(mktemp -d --tmpdir=${BATS_TMPDIR:-/tmp} aardvark_bats.XXXXXX)

    IP_COUNT=0
}

function setup_dnsmasq() {
    command -v dnsmasq || die "dnsmasq not installed"

    run_in_host_netns ip link set lo up
    run_in_host_netns dnsmasq --conf-file=$TESTSDIR/dnsmasq.conf --pid-file="$AARDVARK_TMPDIR/dnsmasq.pid"
    DNSMASQ_PID=$(cat $AARDVARK_TMPDIR/dnsmasq.pid)

    # create new resolv.conf with dnsmasq dns
    echo "nameserver 127.0.0.1" >"$AARDVARK_TMPDIR/resolv.conf"
    run_in_host_netns mount --bind "$AARDVARK_TMPDIR/resolv.conf" /etc/resolv.conf
}

function basic_teardown() {
    # Now call netavark with all the configs and then kill the netns associated with it
    for i in "${!CONTAINER_CONFIGS[@]}"; do
        netavark_teardown $(get_container_netns_path "${CONTAINER_NS_PIDS[$i]}") "${CONTAINER_CONFIGS[$i]}"
        kill -9 "${CONTAINER_NS_PIDS[$i]}"
    done

    if [[ -n "$DNSMASQ_PID" ]]; then
        kill -9 $DNSMASQ_PID
        DNSMASQ_PID=""
    fi

    # Finally kill the host netns
    if [ ! -z "$HOST_NS_PID" ]; then
        echo "$HOST_NS_PID"
        kill -9 "$HOST_NS_PID"
    fi

    rm -fr "$AARDVARK_TMPDIR"
}

################
#  netavark_teardown#  tears down a network
################
function netavark_teardown() {
    run_netavark teardown $1 <<<"$2"
}

function teardown() {
    basic_teardown
}

function dig() {
    # first arg is container_netns_pid
    # second arg is name
    # third arg is server addr
    run_in_container_netns "$1" "dig" "+short" "$2" "@$3" $4
}

function dig_reverse() {
    # first arg is container_netns_pid
    # second arg is the IP address
    # third arg is server addr
    run_in_container_netns "$1" "dig" "-x" "$2" "@$3"
}

function setup() {
    basic_host_setup
}
