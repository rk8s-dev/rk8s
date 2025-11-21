# rk8s - A Lite Version of Kubernetes in Rust

rk8s is a lightweight, Kubernetes-compatible container orchestration system built on top of [Youki](https://github.com/youki-dev/youki), implementing the Container Runtime Interface (CRI) with support for three primary workload types: single containers, Kubernetes-style pods, and Docker Compose-style multi-container applications.

The rk8s project is initiated by the Web3 Infrastructure Foundation and developed with support from Associate Professor Feng Yang’s lab at Nanjing University and the Institute of Software Chinese Academy of Sciences, through the [R2CN](https://r2cn.dev).

The project is currently in **WIP** stage and **should not be used in production environments**.

## Architecture Overview
rk8s follows a distributed architecture with both standalone and cluster deployment modes: 

### Core Components
- **RKL (Container Runtime Interface)** - The primary runtime component supporting CLI operations and daemon mode
- **RKS (Control Plane)** - Kubernetes-like control plane combining API server, scheduler, and controller functionality  
- **Xline** - etcd-compatible distributed storage for cluster state
- **Networking** - CNI-compliant networking with libbridge plugin

## Supported Workload Types

### 1. Single Container Workloads
Manage standalone containers with resource limits and port mappings: 

**Example single container specification:**
```yaml
name: single-container-test
image: ./rk8s/project/test/bundles/busybox
ports:
  - containerPort: 80
    protocol: ""
    hostPort: 0
    hostIP: ""
args:
  - sleep
  - "100"
resources:
  limits:
    cpu: 500m
    memory: 233Mi
```

### 2. Kubernetes-Style Pods
Group multiple containers sharing the same network namespace and lifecycle, implementing the Kubernetes pod model with pause containers for namespace sharing:

**Pod Architecture:**
- Pause container establishes shared namespaces (PID, Network, IPC, UTS)
- Work containers join the pause container's namespaces
- CRI-compliant pod sandbox management
- Resource limits and port mappings per container

**Example pod specification:**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: simple-container-task  
  labels:
    app: my-app 
    bundle: ./rk8s/project/test/bundles/pause
spec:
  containers:
    - name: main-container1    
      image: ./rk8s/project/test/bundles/busybox
      args:             
        - "dd"                   
        - "if=/dev/zero"  
        - "of=/dev/null"          
      ports:
        - containerPort: 80
      resources:
        limits:
          cpu: "500m"
          memory: "512Mi"
status:
```

### 3. Docker Compose-Style Applications

The compose functionality represents rk8s's philosophy of providing familiar developer experiences while maintaining Kubernetes compatibility. This approach bridges the gap between local development workflows and production Kubernetes deployments. 

**Design Philosophy:**
- **Developer Familiarity** - Use Docker Compose syntax that developers already know
- **Kubernetes Compatibility** - Internally translate compose specifications to Kubernetes-compatible pod structures
- **Unified Runtime** - Single runtime handles both Kubernetes pods and Compose applications
- **Progressive Complexity** - Start with simple compose files, migrate to full Kubernetes specs as needed

**Example compose specification:**
```yaml
services:
  backend:
    container_name: back
    image: ./project/test/bundles/busybox
    command: ["sleep", "300"]
    ports:
      - "8080:8080"
    networks:
      - libra-net
    volumes:
      - ./tmp/mount/dir:/app/data

  frontend:
    container_name: front
    command: ["sleep", "300"]
    image: ./project/test/bundles/busybox
    ports:
      - "80:80"

networks:
  libra-net:
    driver: bridge

configs:
  backend-config:
    file: ./config.yaml
```

## Deployment Modes

### Local Mode (Standalone) 
Direct CLI interaction with local container runtime:
- No central control plane required
- Immediate container creation and management
- Ideal for development and testing

### Cluster Mode (Distributed) 
Full Kubernetes-like cluster with distributed components:
- RKS control plane for scheduling and state management
- RKL daemons on worker nodes
- Xline for distributed state storage
- QUIC-based communication between components

## Project Structure

```bash
rk8s/
├── project/
│   ├── rkl/                    # Container Runtime Interface
│   │   ├── src/
│   │   │   ├── commands/       # CLI command implementations
│   │   │   │   ├── compose/    # Compose workload management
│   │   │   │   ├── container/  # Single container operations
│   │   │   │   └── pod/        # Pod lifecycle management
│   │   │   ├── cri/           # CRI API definitions
│   │   │   ├── daemon/        # Daemon mode implementation
│   │   │   ├── task.rs        # Pod task orchestration
│   │   │   └── main.rs        # CLI entry point
│   │   └── tests/             # Integration tests
│   ├── rks/                   # Control plane server
│   ├── libbridge/             # CNI networking plugin
│   └── libipam/              # IP address management
├── docs/                      # Documentation
└── README.md                  # This file
```

## Quick Start

### Prerequisites
- Rust toolchain (1.70+)
- Root privileges for container operations
- Docker (for creating OCI bundles)

### Build and Setup
1. **Build the project:**
```bash
cd rk8s/project/
cargo build -p rkl
cargo build -p rks
```

2. **Set up networking:**
```bash
cargo build -p libbridge
cargo build -p libipam
sudo install -Dm755 target/debug/libbridge /opt/cni/bin/libbridge
sudo install -Dm755 target/debug/libipam /opt/cni/bin/libipam
```

3. **Prepare container images:**
```bash
mkdir -p rootfs
docker export $(docker create busybox) | tar -C rootfs -xvf -
```

### Usage Examples

**Single Container:**
```bash
sudo project/target/debug/rkl container run single.yaml
sudo project/target/debug/rkl container list
sudo project/target/debug/rkl container exec single-container-test /bin/sh
```

**Pod Management(Standalone):**
```bash
sudo project/target/debug/rkl pod run pod.yaml
sudo project/target/debug/rkl pod state simple-container-task
sudo project/target/debug/rkl pod exec simple-container-task container-name /bin/sh
```

**Pod Management(Cluster):**
```bash
# Create pod via RKS control plane
sudo project/target/debug/rkl pod create pod.yaml --cluster 127.0.0.1:50051
# List pods from RKS
sudo project/target/debug/rkl pod list --cluster 127.0.0.1:50051
# Delete pod via RKS
sudo project/target/debug/rkl pod delete simple-container-task --cluster 127.0.0.1:50051
```

**Compose Applications:**
```bash
sudo project/target/debug/rkl compose up
sudo project/target/debug/rkl compose ps
sudo project/target/debug/rkl compose down
```

**Daemon Mode:**
```bash
sudo RKL_ADDRESS=127.0.0.1:50051 project/target/debug/rkl pod daemon # Monitors /etc/rk8s/manifests/ and acts as work node of rks
```

**RKS:**
For detail about building a cluster with RKS and RKL, please refer to [RKS–RKL usage](./project/rks/Readme.md).

## Key Features
- **CRI Compliance** - Full Container Runtime Interface implementation
- **Kubernetes Compatibility** - Pod specifications and resource management
- **Docker Compose Support** - Familiar multi-container application definitions
- **Namespace Sharing** - Proper pod networking with pause containers
- **Resource Management** - CPU and memory limits with cgroups integration
- **CNI Networking** - Pluggable network configuration
- **Daemon Mode** - Static pod reconciliation and monitoring
- **Cluster Orchestration** - Distributed scheduling and state management

## License
rk8s is licensed under this Licensed:

- MIT LICENSE ( [LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)

## Contributing

The rk8s project relies on community contributions and aims to simplify getting started. Pick an issue, make changes, and submit a pull request for community review.

More information on contributing to rk8s is available in the [Contributing Guide](docs/contributing.md).
