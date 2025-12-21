# SlayerFS 元数据后端文件锁单元测试方案

## 概述

本文档提供 SlayerFS 三个元数据存储后端（DatabaseStore、EtcdStore、RedisStore）文件锁功能的完整单元测试方案。

## 测试目标

- 验证三个存储后端的文件锁功能正确性
- 确保锁操作的原子性和一致性
- 测试并发场景下的锁行为
- 验证性能基准

## 环境准备

### 必要服务启动

```bash
# 启动 Redis 服务
docker run -d --name redis-test -p 6379:6379 redis:7-alpine

# 启动 etcd 服务
docker run -d --name etcd-test -p 2379:2379 quay.io/coreos/etcd:v3.5.0 \
  --data-dir=/etcd-data --name node1 \
  --listen-client-urls http://0.0.0.0:2379 \
  --advertise-client-urls http://0.0.0.0:2379
```

### 环境变量配置

创建 `.env.test` 文件：
```bash
REDIS_URL=redis://localhost:6379
ETCD_URL=http://localhost:2379
DATABASE_URL=sqlite:///tmp/test_file_lock.db
```

## 测试执行

### 运行所有测试

```bash
# 运行所有文件锁单元测试
cargo test --lib meta::stores::tests -- --ignored

# 运行特定存储后端测试
cargo test --lib meta::stores::database_store::tests -- --ignored  # Database
cargo test --lib meta::stores::redis_store::tests -- --ignored      # Redis
cargo test --lib meta::stores::etcd_store::tests -- --ignored        # etcd
```

### 运行特定测试场景

```bash
# 基础功能测试
cargo test test_basic_read_lock --lib -- --ignored
cargo test test_multiple_read_locks --lib -- --ignored
cargo test test_write_lock_conflict --lib -- --ignored
cargo test test_lock_release --lib -- --ignored
cargo test test_non_overlapping_locks --lib -- --ignored
```

## 测试覆盖范围

### 1. 基础功能测试 ✅

#### 1.1 基本读锁测试 (`test_basic_read_lock`)
- 验证读锁正确获取
- 验证锁信息查询功能
- 验证锁状态一致性

```rust
#[tokio::test]
#[ignore]
async fn test_basic_read_lock() {
    let store = new_test_store().await;
    let session_id = Uuid::now_v7();
    let inode = 12345;
    let owner = 1001;

    // 设置会话
    store.set_sid(session_id).unwrap();

    // 获取读锁
    store.set_plock(
        inode, owner, false, FileLockType::ReadLock,
        FileLockRange { start: 0, end: 100 }, 1234
    ).await.unwrap();

    // 验证锁存在
    let query = FileLockQuery {
        owner: owner as u64,
        lock_type: FileLockType::ReadLock,
        range: FileLockRange { start: 0, end: 100 },
    };

    let lock_info = store.get_plock(inode, &query).await.unwrap();
    assert_eq!(lock_info.lock_type, FileLockType::ReadLock);
    assert_eq!(lock_info.range.start, 0);
    assert_eq!(lock_info.range.end, 100);
    assert_eq!(lock_info.pid, 1234);
}
```

#### 1.2 多读锁共存测试 (`test_multiple_read_locks`)
- 验证多个读锁可以同时存在
- 验证不同会话的读锁独立性
- 验证锁状态正确查询

#### 1.3 写锁冲突检测 (`test_write_lock_conflict`)
- 验证写锁与读锁的冲突检测
- 验证写锁与写锁的冲突检测
- 验证非阻塞模式立即返回错误

#### 1.4 锁释放测试 (`test_lock_release`)
- 验证锁的正确释放
- 验证释放后锁状态清除
- 验证解锁操作的幂等性

#### 1.5 非重叠锁测试 (`test_non_overlapping_locks`)
- 验证非重叠范围的写锁可以共存
- 验证锁范围检测的正确性
- 验证锁状态查询的准确性

### 2. 边界条件测试

#### 2.1 零长度锁测试
```rust
#[tokio::test]
#[ignore]
async fn test_zero_length_locks() {
    let store = new_test_store().await;
    let session_id = Uuid::now_v7();

    store.set_sid(session_id).unwrap();

    // 测试零长度锁
    let result = store.set_plock(
        inode, owner, false, FileLockType::WriteLock,
        FileLockRange { start: 100, end: 99 }, pid
    ).await;

    // 零长度锁应该被拒绝或特殊处理
    assert!(result.is_err());
}
```

#### 2.2 边界值测试
```rust
#[tokio::test]
#[ignore]
async fn test_boundary_values() {
    // 测试最大范围的锁
    // 测试最小范围的单字节锁
    // 测试负值处理
}
```

### 3. 错误处理测试

#### 3.1 无效 inode 测试
```rust
#[tokio::test]
#[ignore]
async fn test_invalid_inode_operations() {
    let store = new_test_store().await;
    let session_id = Uuid::now_v7();
    store.set_sid(session_id).unwrap();

    // 对不存在的 inode 操作
    let result = store.set_plock(
        -1, owner, false, FileLockType::WriteLock,
        FileLockRange { start: 0, end: 100 }, pid
    ).await;

    assert!(result.is_err());
}
```

#### 3.2 未设置会话测试
```rust
#[tokio::test]
#[ignore]
async fn test_session_not_set() {
    let store = new_test_store().await;
    // 不设置会话直接操作

    let result = store.set_plock(
        inode, owner, false, FileLockType::WriteLock,
        FileLockRange { start: 0, end: 100 }, pid
    ).await;

    assert!(result.is_err());
}
```

### 4. 性能基准测试

#### 4.1 吞吐量测试
```rust
#[tokio::test]
#[ignore]
async fn benchmark_lock_throughput() {
    let store = Arc::new(new_test_store().await);
    let file_ino = prepare_test_file(&store).await;

    let operations = 1000;
    let start = Instant::now();

    for i in 0..operations {
        let session_id = Uuid::now_v7();
        store.set_sid(session_id).unwrap();

        store.set_plock(
            file_ino, i as i64, false, FileLockType::ReadLock,
            FileLockRange { start: i * 10, end: i * 10 + 9 }, pid
        ).await.unwrap();
    }

    let duration = start.elapsed();
    let ops_per_sec = operations as f64 / duration.as_secs_f64();

    println!("吞吐量: {:.2} ops/sec", ops_per_sec);
    assert!(ops_per_sec > 100.0, "吞吐量应该大于 100 ops/sec");
}
```

#### 4.2 延迟测试
```rust
#[tokio::test]
#[ignore]
async fn benchmark_lock_latency() {
    let store = Arc::new(new_test_store().await);
    let file_ino = prepare_test_file(&store).await;

    let iterations = 1000;
    let mut latencies = Vec::new();

    for i in 0..iterations {
        let session_id = Uuid::now_v7();
        store.set_sid(session_id).unwrap();

        let start = Instant::now();

        store.set_plock(
            file_ino, i % 100, false, FileLockType::ReadLock,
            FileLockRange { start: 0, end: 100 }, pid
        ).await.unwrap();

        let latency = start.elapsed();
        latencies.push(latency.as_micros());

        // 立即释放
        store.set_plock(
            file_ino, i % 100, false, FileLockType::UnLock,
            FileLockRange { start: 0, end: 100 }, pid
        ).await.unwrap();
    }

    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() * 95) / 100];

    println!("P50 延迟: {} μs, P95 延迟: {} μs", p50, p95);
    assert!(p50 < 5000, "P50 延迟应小于 5ms");
}
```

## 测试自动化

### 自动化脚本

创建 `scripts/run_file_lock_tests.sh`：

```bash
#!/bin/bash
set -e

echo "=== SlayerFS 元数据后端文件锁测试 ==="

# 检查 Docker 服务
if ! docker ps | grep -q redis-test; then
    echo "启动 Redis 测试服务..."
    docker run -d --name redis-test -p 6379:6379 redis:7-alpine
    sleep 2
fi

if ! docker ps | grep -q etcd-test; then
    echo "启动 etcd 测试服务..."
    docker run -d --name etcd-test -p 2379:2379 quay.io/coreos/etcd:v3.5.0 \
        --data-dir=/etcd-data --name node1 \
        --listen-client-urls http://0.0.0.0:2379 \
        --advertise-client-urls http://0.0.0.0:2379
    sleep 3
fi

echo "运行 DatabaseStore 文件锁测试..."
cargo test --lib meta::stores::database_store::tests -- --ignored

echo "运行 RedisStore 文件锁测试..."
cargo test --lib meta::stores::redis_store::tests -- --ignored

echo "运行 EtcdStore 文件锁测试..."
cargo test --lib meta::stores::etcd_store::tests -- --ignored

echo "运行性能基准测试..."
cargo test benchmark --lib -- --ignored

echo "=== 测试完成 ==="

# 清理选项（注释掉如果需要保留服务）
# docker stop redis-test etcd-test
# docker rm redis-test etcd-test
```

### Docker Compose 配置

创建 `tests/docker-compose.test.yml`：

```yaml
version: '3.8'

services:
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    command: redis-server --appendonly yes

  etcd:
    image: quay.io/coreos/etcd:v3.5.0
    ports:
      - "2379:2379"
      - "2380:2380"
    environment:
      - ETCD_AUTO_COMPACTION_MODE=revision
      - ETCD_AUTO_COMPACTION_RETENTION=1000
    command:
      - /usr/local/bin/etcd
      - --data-dir=/etcd-data
      - --name node1
      - --initial-advertise-peer-urls http://0.0.0.0:2380
      - --listen-peer-urls http://0.0.0.0:2380
      - --advertise-client-urls http://0.0.0.0:2379
      - --listen-client-urls http://0.0.0.0:2379
      - --initial-cluster node1=http://0.0.0.0:2380
```

使用 Docker Compose 启动：
```bash
docker-compose -f tests/docker-compose.test.yml up -d
```

## 预期结果

### 成功标准

1. **DatabaseStore** (SQLite/PostgreSQL)
   - ✅ 所有基础功能测试通过
   - ✅ 边界条件测试通过
   - ✅ 错误处理正确
   - ⏱ 吞吐量: >100 ops/sec
   - ⏱ P50 延迟: <10ms

2. **RedisStore**
   - ✅ 所有基础功能测试通过
   - ✅ 高并发性能良好
   - ⏱ 吞吐量: >1000 ops/sec
   - ⏱ P50 延迟: <1ms

3. **EtcdStore**
   - ✅ 所有基础功能测试通过
   - ✅ 分布式一致性正确
   - ⏱ 吞吐量: >50 ops/sec
   - ⏱ P50 延迟: <50ms

### 故障排查

#### Redis 连接失败
```bash
# 检查 Redis 服务
docker ps | grep redis
docker logs redis-test

# 手动测试连接
redis-cli -h localhost -p 6379 ping
```

#### etcd 连接失败
```bash
# 检查 etcd 服务
docker ps | grep etcd
docker logs etcd-test

# 手动测试连接
etcdctl --endpoints=http://localhost:2379 endpoint health
```

#### 测试超时
```bash
# 增加测试超时时间
export RUST_TEST_TIMEOUT=300
cargo test --lib meta::stores::tests -- --ignored
```

## 持续集成集成

### GitHub Actions 配置

```yaml
name: File Lock Meta Store Tests

on: [push, pull_request]

jobs:
  test-meta-stores:
    runs-on: ubuntu-latest

    services:
      redis:
        image: redis:7-alpine
        ports:
          - 6379:6379
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

      etcd:
        image: quay.io/coreos/etcd:v3.5.0
        ports:
          - 2379:2379
        options: >-
          --health-cmd "etcdctl endpoint health"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
    - uses: actions/checkout@v3
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable

    - name: Run file lock tests
      env:
        REDIS_URL: redis://localhost:6379
        ETCD_URL: http://localhost:2379
      run: |
        cargo test --lib meta::stores::tests -- --ignored
```

这个测试方案提供了全面的元数据后端文件锁功能验证，确保 SlayerFS 在三个不同存储后端上的文件锁功能正确性和性能。