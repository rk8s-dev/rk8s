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

#### XlineMetaStore（分布式 KV 存储后端）

- **技术栈**：基于 etcd-client，通过 Xline/etcd 集群提供分布式协调
- **关键特性**：
  - 基于分布式 KV 存储实现横向扩展
  - 多实例间保持元数据一致性
  - 面向高可用部署优化

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
- **缓存集成**：内存 `Namespace` 作为直写缓存层

### 工具与示例

- **持久化演示工具（`persistence_demo.rs`）**

  - 多后端配置切换

  - 挂载/卸载完整流程

  - 使用示例

    ```sh
    cargo run --example persistence_demo -- --config config.yaml
    ```

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
```

​	目前，本地数据库模式下，在挂载后的文件夹中，可以正常操作文件夹，包括重命名，删除，读写文件之类的操作。