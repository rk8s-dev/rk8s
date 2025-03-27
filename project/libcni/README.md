# A Library For CNI-Plugins

This project provides functionality to set up `veth` network interfaces between containers and hosts, as well as handling network namespaces (`netns`). It includes support for creating and managing network namespaces, linking interfaces, and configuring veth devices in these namespaces.

## Files Overview

- **`veth.rs`**: Contains the implementation of the `Veth` struct, which represents a pair of virtual Ethernet devices (`veth`). It includes methods to configure, set MAC addresses, and convert to different representations for CNI responses or network configuration.
- **`ns.rs`**: Contains the implementation of the `Netns` struct, which represents a network namespace. It includes methods to create, get, set, and delete namespaces, as well as managing the network namespace file descriptors.
- **`link.rs`**: Implements network interface linking, setting up interfaces, and managing interface configurations.

## Key Concepts

### Veth Pair

A **veth pair** consists of two network interfaces, typically used to create a link between the host and a container. These interfaces are named (e.g., `veth0` for the container side and `veth1` for the host side), and the pair behaves like a virtual network cable, where data sent on one interface will be received on the other.

### Network Namespace

A **network namespace** (`netns`) is an isolated network environment where network interfaces, routing tables, and other networking aspects are isolated from the host system and other containers. Each container can be placed in its own network namespace to prevent conflicts between container networks.

---

## Usage

### Create a Network Namespace

To create a new network namespace:

```rust
use ns::Netns;

let ns = Netns::new().expect("Failed to create network namespace");
```

### Create a Named Network Namespace

You can create a network namespace with a specific name:

```rust
let ns = Netns::new_named("my_namespace").expect("Failed to create named network namespace");
```

### Delete a Network Namespace

To delete a previously created named namespace:
```rust
Netns::delete_named("my_namespace").expect("Failed to delete network namespace");
```

### Execute Commands in a Network Namespace

To execute a function in a specific network namespace:
```rust
let result = exec_netns(&current_ns, &target_ns, async {
    // Execute operations in target_ns
    Ok(())
}).await;
```

### Create a Veth Pair

To create a veth pair between a container and the host:

```rust
let container_ns = Netns::new().expect("Failed to create container namespace");
let host_ns = Netns::get().expect("Failed to get host network namespace");

let container_veth_name = "veth0";
let host_veth_name = "veth1";
let mtu = 1500;
let container_mac = MacAddr::new([0x00, 0x15, 0x5D, 0x1A, 0x1F, 0x1C]);

let veth = setup_veth(
    container_veth_name,
    host_veth_name,
    mtu,
    &container_mac,
    &host_ns,
    &container_ns,
).await.expect("Failed to set up veth pair");
```

---

## Example Flow

- **Create network namespaces** for both the container and host.
- **Create a veth pair** between the container and host with specific configurations (MAC address, MTU).
- **Set the namespace** for the target container or host interface.
- **Perform network operations** within the isolated namespaces.

## Error Handling

All functions in this project use the `anyhow` crate to return detailed error messages. In case of errors, appropriate messages are logged and returned for easy debugging.

## Dependencies

- `nix`:For namespace and mount management.
- `rtnetlink`:For interacting with network devices.
- `rand`:For generating random veth names.
- `log`:For logging.