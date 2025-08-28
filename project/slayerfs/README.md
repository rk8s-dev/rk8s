
# SlayerFS: High-performance Rust & FUSE-based S3 Container Filesystem

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![FUSE](https://img.shields.io/badge/FUSE-3.0-green.svg)](https://github.com/libfuse/libfuse)

## ✨ Project Overview

SlayerFS is an innovative AI- and container-focused distributed filesystem designed to serve as a drop-in replacement for traditional container filesystems. It implements intelligent storage scheduling and management at the filesystem layer. Using FUSE, SlayerFS can transparently mount S3-compatible object storage (or other backends) as a container's root filesystem or data layer, changing how containers access data.

Core idea: containers do not need to know where data is stored. SlayerFS' scheduling layer automatically decides where data lives (local caches, remote S3) and how it is accessed, enabling true separation of compute and storage and elastic scaling.

Built in Rust, SlayerFS leverages Rust's memory safety and high-performance concurrency to let containerized applications access distributed storage resources via standard POSIX file operations.

## 🚀 Quick Start

### Requirements

- Rust: >= 1.75.0
- Operating system: Linux (Ubuntu 20.04+, CentOS 8+)

```bash
cargo run -q --bin sdk_demo -- /tmp/slayerfs-objroot
```
---

## 🌟 Key Features

### Container filesystem replacement
- Transparent replacement: fully replaces a container's traditional filesystem without requiring application changes.
- Intelligent scheduling: the filesystem layer automatically chooses storage locations and access policies, enabling seamless integration with image and storage backends.
- Compute-storage separation: compute and storage are decoupled, enabling stateless containers.
- Elastic scalability: storage capacity and performance scale independently from compute.

### Intelligent storage scheduling
- Multi-tier storage: local memory/SSD caches combined with remote S3 storage in a tiered architecture.
- Hot data detection: identify hot data based on access patterns.
- Predictive prefetching: AI-driven prefetch strategies.
- Dynamic migration: automatic movement of data between storage tiers.

### High-performance caching
- Multi-layer cache design: memory cache + disk cache + remote storage.
- Smart prefetching driven by access patterns.
- Concurrency optimizations: async I/O and multi-threading.
- Configurable cache invalidation and consistency policies.

### Deep container ecosystem integration
- Root filesystem replacement: usable as a container's root filesystem.
- overlayfs compatible: supports layered container filesystems.
- CSI driver support: integrates with Kubernetes CSI.
- Container runtime integration: works with Docker, containerd, and CRI-O.

### Enterprise features
- Monitoring integration: Prometheus metrics support.
- Logging: structured logs and tracing.
- Fault recovery: automatic reconnect and error handling.
- Configuration: YAML/TOML configuration support.

---

## 🎯 Use Cases

### Modernizing container storage
- Stateless containers: remove reliance on local container disk.
- Faster startup: containers do not need to download full image data on start.
- Storage elasticity: capacity scales independently from compute.
- Shared data across replicas: multiple container instances can share the same dataset.

### AI/ML workload optimization
- Model-as-a-service: mount trained models directly via the filesystem.
- Dataset virtualization: transparent access to TB-scale datasets without local copies.
- Training acceleration: hot data cached to fast SSDs automatically.
- Model versioning: quick switches between model versions.

### Microservices modernization
- Centralized configuration: application configuration stored in S3.
- Dynamic dependency loading: runtime-on-demand loading of dependencies.
- Log archiving: application logs automatically archived to object storage.
- Static asset serving: transparent access to web assets.

### Data lakes and big data
- Direct data lake access for big data applications.
- ETL optimization: improved I/O for batch jobs.
- Transparent access to archived data.
- Multi-tenant isolation via path-based separation.

### Edge computing
- Edge caching: intelligent caching at edge nodes.
- Centralized storage of results from edge workloads.
- Bandwidth-aware scheduling for transfers.
- Offline capability: local cache access during network outages.

---


## 🤝 Contributing

Contributions of all kinds are welcome!


