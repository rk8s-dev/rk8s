# Docker Compose Testing Setup

Several SlayerFS cargo tests depend on storage services (PostgreSQL, etcd, Redis). This document describes how to quickly set up these services using Docker Compose or Podman Compose.

## Installation

### Debian/Ubuntu

```bash
# Docker
sudo apt update && sudo apt install docker.io docker-compose

# Podman
sudo apt update && sudo apt install podman podman-compose
```

### Arch Linux

```bash
# Docker
sudo pacman -S docker docker-compose

# Podman
sudo pacman -S podman podman-compose
```

### macOS

```bash
# Docker
brew install docker docker-compose

# Podman
brew install podman podman-compose
```

## Usage

### Docker

```bash
# Start services in background
docker compose up -d

# Run tests
cargo test --lib meta::stores::redis_store -- --nocapture
cargo test --lib meta::stores::etcd_store -- --nocapture
cargo test --lib meta::stores::database_store -- --nocapture

# Stop services when done
docker compose down
```

### Podman

```bash
# Start services in background
podman-compose up -d

# Run tests (same commands as above)
cargo test --lib meta::stores::redis_store -- --nocapture
cargo test --lib meta::stores::etcd_store -- --nocapture
cargo test --lib meta::stores::database_store -- --nocapture

# Stop services when done
podman-compose down
```

## Alternative

For Docker users, the automated test script `tests/scripts/test_meta_store.sh` handles service lifecycle automatically.
