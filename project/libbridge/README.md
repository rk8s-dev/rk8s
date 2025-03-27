# CNI Plugin Bridge Network Configuration

This project provides a CNI plugin for configuring and managing bridge network interfaces in containerized environments. It includes functionality for managing VLANs, handling errors, and supporting custom network configurations for bridges.

## Features

- **Error Handling**: Custom error types (`AppError` and `VlanError`) are used for detailed and structured error reporting, which can be translated into CNI error replies.
- **Bridge Network Configuration**: The `BridgeNetConf` struct provides an extensive configuration model for setting up bridge networks, including options for VLAN support, MAC address filtering, and various network interface options.
- **VLAN Trunking**: Support for defining VLAN trunking ranges (`VlanTrunk`) and VLAN filtering in bridge configurations.
- **Serialization**: The project uses Serde to serialize and deserialize configurations, ensuring flexibility in handling network configuration files.
- **Bridge Management**: The `Bridge` struct offers an abstraction for a network bridge, including methods for setting MTU size and enabling VLAN filtering.

## Components

### Error Handling

The project defines two main error types:

- **AppError**: Covers general errors, including CNI, VLAN, network namespace, and other related issues.
- **VlanError**: Specifically handles errors related to VLAN configuration, including invalid trunk IDs, missing parameters, and incorrect ID ranges.

Both error types can be converted into CNI-compatible error replies.

### Bridge Network Configuration

The `BridgeNetConf` struct extends `NetworkConfig` and contains options for configuring a bridge network, such as:

- Bridge name and gateway settings.
- MAC address spoofing and DAD (Duplicate Address Detection).
- VLAN configuration, including trunking and filtering.

Additional configurations like MTU size and port isolation are also supported.

### Bridge Struct

The `Bridge` struct is a representation of a virtual network bridge, with methods to set the MTU size and enable VLAN filtering. This struct can be converted into a `LinkMessageBuilder` for further use in network setup.

### Example 

- **Configuration**: my-bridge.conf
```json
{
  "cniVersion": "0.3.1",
  "name": "my-bridge",
  "type": "bridge",
  "bridge": "br0",
  "isGateway": true,
  "mtu": 1500,
  "vlan": 100,
  "vlanTrunk": [
    { "minID": 10, "maxID": 20 },
    { "id": 30 }
  ]
}
```
- **Shell**
```shell
#set up network namespace
sudo ip netns add test

# add bridge
> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool add my-bridge /var/run/netns/test

# del bridge
> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool del my-bridge /var/run/netns/test
```

### Dependencies

- **CNI Plugin**: The main plugin for handling CNI-specific errors and replies.
- **Serde**: For serializing and deserializing configuration data.
- **Rtnetlink**: For interacting with Linux network interfaces (e.g., setting up bridges and VLANs).
