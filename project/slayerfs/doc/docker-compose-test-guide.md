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

## Version Compatibility

The commands in this guide use `docker compose` (without hyphen), which is the modern Docker Compose V2 syntax. This is available in:
- Docker Desktop 4.10+ (includes Compose V2)
- Docker Engine with the `docker-compose-plugin` package installed

For older installations using the standalone `docker-compose` binary (with hyphen), replace `docker compose` with `docker-compose` in all commands.

Example for older versions:
```bash
# Start services (older docker-compose v1 syntax)
docker-compose up -d

# Stop services
docker-compose down
```

Both `podman-compose` and `podman compose` syntaxes are supported.

## Security Notice

**WARNING**: The services in this configuration use default credentials that are only suitable for local development:
- **PostgreSQL**: username `slayerfs`, password `slayerfs`
- **etcd**: No authentication
- **Redis**: No authentication

These credentials are **NOT secure** and must NEVER be used in production or any shared/accessible environment. Always use strong, unique credentials and proper authentication mechanisms for production deployments.

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
