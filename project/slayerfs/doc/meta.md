# SlayerFS 元数据持久化架构设计说明

## 概览

本文档描述了 SlayerFS 引入的元数据持久化层架构。该实现通过统一抽象层（`MetaStore` trait），支持多种后端存储系统，从而实现灵活的部署模式，满足不同基础设施环境下的需求。

## 架构设计

### MetaStore 抽象层

`MetaStore` trait（定义于 `src/meta/store.rs`）提供了统一的元数据操作接口，将具体存储实现与文件系统逻辑解耦。

#### 接口定义

该 trait 定义了文件系统的核心操作：

- **文件/目录管理**：`stat`、`lookup`、`readdir`、`mkdir`、`create_file`
- **修改操作**：`unlink`、`rmdir`、`rename`、`set_file_size`
- **路径解析**：`lookup_path` 用于高效路径到 inode 转换
- **初始化**：`initialize`、`root_ino` 用于系统引导

#### 设计优势

- **后端无关**：上层（VFS、FUSE）不依赖具体存储实现
- **可扩展性**：实现 trait 即可接入新的存储后端（TiKV、Redis 等）
- **类型安全**：所有操作均返回结构化错误（`MetaError`），便于处理

### 后端实现

#### DatabaseMetaStore（关系型数据库后端）

- **技术栈**：基于 SeaORM，支持 SQLite 与 PostgreSQL，提供类型安全的数据库操作
- **Schema 设计**：三张核心表：
  - `access_meta`：目录权限、时间戳、访问控制
  - `file_meta`：文件大小、权限、时间戳及文件元数据
  - `content_meta`：父子关系、目录项名称、目录结构
- **关键特性**：
  - SeaORM 宏生成类型安全查询
  - 自动迁移支持
  - 针对关系查询的索引优化
  - 事务保证 ACID 特性

#### EtcdMetaStore（分布式 KV 存储后端）

- **技术栈**：基于 etcd-client，通过 Xline/etcd 集群提供分布式协调
- **关键特性**：
  - 基于分布式 KV 存储实现横向扩展
  - 多实例间保持元数据一致性
  - 面向高可用部署优化

#### RedisMetaStore（高性能 KV 存储后端）

- **技术栈**：使用 `redis` 异步连接管理器，复用 Redis 的单线程原子语义（`INCR`、Lua/事务）保证一致性
- **键空间规划**：采用与 JuiceFS 类似的命名约定，`i{ino}` 存储 inode JSON、`d{ino}` 维护目录子项、`c{ino}_{chunk_idx}` 记录切片列表、`nextinode`/`nextchunk` 负责全局 ID、`delslices` 存待清理记录

### 数据模型与实体

- **实体模型**：基于 SeaORM 宏生成类型安全 ORM 实体
- **权限模型**：类 Unix 权限系统（`permission.rs`），支持 mode、uid、gid
- **序列化**：实体支持 `serde`，实现跨后端兼容

### 配置管理

- **配置格式**：基于 YAML 的配置系统（`config.rs`）
- **工厂模式**：`factory.rs` 提供统一实例化方法（支持 URL 或配置文件）
- **运行时切换**：可根据配置选择不同后端

示例：

```yaml
database:
  type: sqlite
  url: "sqlite:///tmp/slayerfs/metadata.db"
```

### VFS 集成

- **依赖注入**：VFS 构造函数接受 `MetaStore` 实例，解耦具体实现
- **持久化操作**：写操作（如 `mkdir_p`、`create_file`）会持久化至后端
- **延迟加载**：`readdir` 按需加载目录内容

### 工具与示例

- **持久化演示工具（`persistence_demo.rs`）**

  - 多后端配置切换

  - 挂载/卸载完整流程

  - 使用示例

    ```sh
    cargo run --example persistence_demo -- --config config.yaml
    ```

### 元数据缓存实现 

​	`MetaClient`作为元数据客户端代理，实现了一个高性能、高并发、支持 TTL/LRU 策略的元数据缓存层，旨在加速底层元数据存储（MetaStore）的访问性能。其核心设计目标是提供快速的路径解析、Inode 属性查询和目录列表读取，同时通过一套高效的缓存失效机制来保证数据的一致性。

#### 2. 核心架构

缓存系统采用分层、多策略的复合架构，主要由以下三个核心组件构成：

- **Inode 缓存 (InodeCache):** 负责缓存文件和目录的元数据属性（FileAttr）、父子关系及目录内容。
- **路径解析缓存 (Path Cache & PathTrie):** 负责缓存从绝对路径到 Inode 编号的映射关系。
- **缓存失效机制:** 一套基于前缀树和反向索引的精确、高效的缓存失效策略。

#### 3. 关键组件详解

**3.1. Inode 缓存 (InodeCache)**

- **双层存储模型:** 采用 `Moka Cache` 和 `DashMap` 结合的双层设计。
  - **Moka:** 作为生命周期管理器，负责实现基于 TTL（存活时间）和 LRU（最近最少使用）的缓存淘汰策略。
  - **DashMap:** 作为主存储，提供对` InodeEntry `的高并发、分片锁读写访问，允许在不影响 Moka 内部结构的情况下对缓存项进行原地修改。
- **状态化目录内容 (ChildrenState):** 通过` NotLoaded`、`Partial`、`Complete` 三种状态精确追踪目录内容的缓存状态,确保了只有从后端完整加载过的目录（Complete 状态）才会被用于响应 readdir 请求，有效避免了数据不完整问题。
- **数据结构:** `InodeEntry` 封装了单个 Inode 的所有缓存信息，包括属性 (attr)、父节点指针 (parent) 和子节点列表 (children)，均采用 Arc<RwLock<T>> 进行并发保护。

**3.2. 路径解析缓存**

为实现高效的路径解析和失效，系统同时使用了两种数据结构：

- **直接路径缓存 (path_cache):** 一个基于 Moka 的 K-V 缓存，用于存储完整路径字符串到 Inode 编号的直接映射。它为已解析路径提供了 O(1) 的访问速度。
- **路径前缀树 (PathTrie):** 一个专用于路径管理的前缀树.结构的核心优势在于支持**前缀匹配失效**。当一个目录被修改时（如重命名或删除），可以 O(depth) 的复杂度移除整个子树对应的所有路径缓存，相比于遍历扁平化缓存（O(N)），效率极高。

**3.3. 缓存失效策略**

缓存失效是保证数据一致性的关键。本系统设计了一套精准、高效的失效流程：

1. **反向索引 (inode_to_paths):** 维护一个从 Inode 到其所有路径的 DashMap 映射，可在 O(1) 时间内定位到特定 Inode 对应的所有路径。
2. **级联失效:** 当目录发生写操作时，触发 `invalidate_parent_path` 函数。
3. **执行流程:**
   a. 使用**反向索引**找到被修改目录的所有路径。
   b. 对每个路径，调用 **PathTrie::remove_by_prefix** 方法，原子性地移除该路径及其所有子孙路径。
   c. remove_by_prefix 返回所有被移除的路径-Inode 对。
   d. 遍历这些被移除的条目，从**直接路径缓存 (path_cache)** 和**反向索引 (inode_to_paths)** 中清理对应的数据，从而完成整个级联失效过程。

## TODO：

### 高优先级（P0）

1. 完善metastore设计与现有store的实现
2. 引入事务支持（数据库与 etcd）
3. 实现元数据和后端存储的对应关系

### 中优先级（P1）

1. 优化性能与大目录可扩展性
2. 实现权限系统（包括 FUSE 映射）
3. 增强错误处理（消除字符串化错误）

### persistence_demo

本机运行示例：

```sh
# 使用sqlite持久化存储元数据，将文件内容块儿保存在/tmp/sqlite文件夹。
# 挂载的视图展示在/tmp/mount，也就是直接在/tmp/mount进行操作
cargo run --example persistence_demo -- -c sqlite.yml -s /tmp/sqlite -m /tmp/mount
```

sqlite.yml

```yml
database:
  type: sqlite
cache:
  enabled: true
  
  capacity:
    inode: 10000          # Inode metadata cache (includes attr, children, parent)
    path: 5000            # Path resolution cache
  
  # Cache TTL (in seconds, supports decimal values)
  ttl:
    inode_ttl: 10.0       # 10 seconds for inode metadata
    path_ttl: 10.0        # 10 seconds for path resolution

```

本机运行示例：

```sh
# 使用PostgreSQL持久化存储元数据，将文件内容块儿保存在/tmp/pg文件夹。
# 挂载的视图展示在/tmp/mount，也就是直接在/tmp/mount进行操作
cargo run --example persistence_demo -- -c pg.yml -s /tmp/pg -m /tmp/mount
```

pg.yml

```yml
database:
  type: postgres
  url: "postgresql://postgres:postgres@127.0.0.1:5432/meta"
cache:
  enabled: true
  
  capacity:
    inode: 10000          # Inode metadata cache (includes attr, children, parent)
    path: 5000            # Path resolution cache
  
  # Cache TTL (in seconds, supports decimal values)
  ttl:
    inode_ttl: 10.0       # 10 seconds for inode metadata
    path_ttl: 10.0        # 10 seconds for path resolution
```

本机运行 Redis 示例：

```sh
# 使用 Redis 持久化存储元数据
cargo run --example persistence_demo -- -c redis.yml -s /tmp/redisdata -m /tmp/mount
```

redis.yml

```yml
database:
  type: redis
  url: "redis://127.0.0.1:6379/0"
cache:
  enabled: true
```

 目前，本地数据库模式下，在挂载后的文件夹中，可以正常操作文件夹，包括重命名，删除，读写文件之类的操作。

运行scripts/cache_demo.sh 后可以在log中看到关于缓存命中、加入、移除等信息