# SlayerFS Status

> 更新时间：2026-02-09  
> 依据：SlayerFS 以本仓库当前实现与文档为准；JuiceFS 以官方文档（Community + CSI 文档）为准。

本文档将SlayerFS和目前的成熟分布式文件系统JuiceFS进行对比，从而说明SlayerFS的现状，并为后续SlayerFS的完善提供依据和路线。

## 1. 总体评价矩阵

| 维度 |  差距结论                                                                         |
|---|------------------------------------------------------------------------------|
| 一致性语义 |  SlayerFS 单实例语义较清晰，但跨客户端一致性仍无强保证；JuiceFS 提供 close-to-open 与一致性边界说明。           |
| 缓存层级  | SlayerFS 已有元数据/读写缓存雏形；JuiceFS 已形成元数据 + 本地内存/磁盘 + 预读/回写策略体系。                  |
| 元数据管理  | SlayerFS 有统一抽象和多后端；JuiceFS 的元数据引擎选型、容量规划、生产建议更成熟。                            |
| 读写路径  | SlayerFS 已实现 chunk/block/slice + 异步提交；JuiceFS 数据路径与缓存策略更完整且长期验证。             |
| 故障恢复  | SlayerFS 具备 session 基础能力，但运维工具链不完整；JuiceFS 提供 dump/load、fsck、gc、status 等。 |
| 扩展性  | SlayerFS 具备分层扩展潜力；JuiceFS 已支持大规模客户端挂载并有成熟部署实践。                               |
| 部署与运维成本 | SlayerFS 当前更偏研发部署；JuiceFS 在 K8s/CSI/生产建议方面更为成熟。                       |

## 2. 具体功能差距

| 功能项 | JuiceFS | SlayerFS                                          |
|---|---|---------------------------------------------------|
| 细粒度权限管理（ACL） | 提供 POSIX ACL 管理（`setfacl/getfacl`）与权限行为说明 | 仅有 mode/uid/gid + ACL 数据结构；ACL 语义在 `MetaStore` 侧仍是预留接口 |
| 目录配额 / 用户配额 / 组配额 | 提供 `quota` 能力与配额告警/校验工具 | `MetaStore` 的 quota 相关接口大多仍为 `NotImplemented` |
| xattr 扩展属性一致性 | 支持用户态 xattr（含容量限制与用法说明） | 仅部分后端（Database）实现 xattr；多后端行为未对齐                  |
| 维护命令工具链（status/gc/fsck/warmup） | 提供完整运维命令用于巡检、回收、修复、预热 | 当前缺少统一 CLI 工具链                                    |
| 备份恢复（metadata dump/load） | 官方提供元数据导出/导入流程 | 暂无等价的一体化命令与标准流程                                   |
| K8s CSI 生产集成 | 提供成熟 CSI 文档与生产实践（StorageClass、动态供给等） | 暂无CSI                                    |
| 生产化元数据选型手册 | 官方明确 Redis/MySQL/PostgreSQL/TiKV 等选型建议与容量规划 | 支持多后端，但文档与实践仍不足                        |
| 跨客户端缓存一致性策略 | 对一致性边界与缓存行为有明确文档与实践经验 | 已有本地缓存失效机制，但跨客户端一致性仍缺强保证        |

## 3. 不同维度对比

### 3.1 一致性语义

**SlayerFS**

- 单进程语义明确：读前刷写、写后异步提交、已提交 slice 对读可见。
- `truncate` 过程有显式 flush + cache 失效，确保单实例句柄内可见性。
- 对多进行提供close-to-open一致性语义，但是没有测试提供的强保证。

**JuiceFS（官方文档）**

- 默认强调 close-to-open 语义，在对象存储强一致前提下可满足多数 POSIX 场景。
- 官方同时公开“一致性例外”（例如并发覆盖、只读副本缓存窗口等），边界清晰。

### 3.2 缓存层级（用户 + 运维）

**SlayerFS**

- 元数据缓存：拥有 inode/path cache + trie 前缀失效机制 + reverse index。
- 读缓存：按文件句柄维护 reader，会做本地 invalidate 与预读窗口管理。
- 写缓存：page/slice 缓存，异步上传 + commit 循环。
- 缺少额外的配置选项来控制不同缓存机制的启用。

**JuiceFS**

- 明确区分 metadata cache、data cache，并支持多种模式（如 open-cache、writeback）。
- 提供预读与本地磁盘缓存策略，支持在 Kubernetes 场景中进行挂载点共享缓存优化。
- 文档中对一致性与缓存命中行为有清晰说明，便于线上调优。

总的来说，SlayerFS已经有基本的缓存系统结构，但是JuiceFS提供更多的选择，比如可以根据不同的场景选择不同的参数。

### 3.3 元数据管理

**SlayerFS**

- 有统一 `MetaStore` 抽象，支持 Database/Redis/Etcd 实现。
- 支持 MetaClient 层缓存与会话管理；Etcd watch 可用于缓存失效事件传播。
- trait 中还有大量的预留功能还未实现，不同后端对于同样操作的表现可能有所不同。

**JuiceFS**

- 元数据引擎支持更多种类（如 Redis、MySQL、PostgreSQL、TiKV 等），并提供容量和性能取舍建议。
- 官方给出明确容量估算方法和生产部署建议。

### 3.4 读写路径

**SlayerFS**

- 写：按 chunk/block/slice 拆分，slice 冻结后异步上传，随后按 chunk 提交元数据。
- 读：按 chunk 拉取并缓存，局部失效由 writer 提交后触发。
- 当前模型对吞吐路径已较清晰，但仍以单实例/本地缓存协同为主，跨客户端可见性仍依赖后续完善。

**JuiceFS**

- 写路径支持对象块上传 + 元数据更新，并支持 writeback 场景下的前置返回策略。
- 读路径支持 metadata 定位 + block cache + 预读策略，线上行为更可预测。

SlayerFS 在数据路径机制上可用，但是在复杂场景和稳定性上仍需进一步的测试和打磨。

### 3.5 故障恢复

**SlayerFS**

- 具备 session 生命周期与清理循环。
- 目前缺少类似 `fsck`、`dump/load`、`status` 之类的运维命令。

**JuiceFS**

- 提供 metadata dump/load 备份恢复方案。
- 提供 `fsck`、`gc`、`status`、`warmup` 等维护能力，生产建议文档完善。

### 3.6 扩展性

**SlayerFS**

- 架构上已经计算/存储解耦，元数据与对象存储均可横向扩展。
- 但多客户端一致性和控制面能力尚在完善，规模化上线风险仍偏高。

**JuiceFS**

- 官方文档给出大规模客户端挂载能力说明，且长期有生产实践沉淀。
- 元数据与对象存储可独立扩缩容，并有 Kubernetes CSI 生态支撑。

### 3.7 部署与运维成本

**SlayerFS**

- 当前部署文档更多偏研发测试（例如 docker-compose 搭建 PostgreSQL/etcd/Redis）。
- 生产部署所需的安全基线、HA 方案、自动化发布与升级手册仍需补全。

**JuiceFS**

- 有较成熟的生产部署建议、元数据高可用建议、Kubernetes CSI 接入路径。
- 组件边界与运维职责更清晰，团队上手成本更低。

## 4. 后续待办

1. 完整性和可用性
    - 明确SlayerFS提供的跨客户端一致性语义，并根据这个语义进行测试保证SlayerFS的数据一致性、完整性和可用性（高优先级）。
    - 保证所有可选的元数据存储可以稳定通过 pjdfstests 和 xfstests 等 POSIX 兼容性测试套件，进一步完善 SlayerFS 的 POSIX 兼容性（高优先级）。
    - 实现 Slice Compaction 功能，保证 SlayerFS 可以长期运行。
2. 功能增强
    - 遵循 POSIX 标准实现 ACL 权限管理机制和 xattr 支持（中优先级）。
    - 提供一套完整的命令行工具对SlayerFS进行监控和管理（中优先级）。
    - 针对 SlayerFS 提供一套 K8S CSI 实现（中优先级）。
    - 为 SlayerFS 提供更多的后端对象存储和元数据存储支持（低优先级）。
    - 为 SlayerFS 实现数据加密功能，保证安全性（低优先级）。

3. 性能增强
    - 为 SlayerFS 实现透明压缩功能，可有效降低对象存储的带宽使用（中优先级）。

---

## 参考资料

### SlayerFS

- `README_CN.md`
- `doc/arch.md`
- `doc/meta.md`
- `doc/bench.md`
- `doc/docker-compose-test-guide.md`
- `src/vfs/fs.rs`
- `src/vfs/io/writer.rs`
- `src/vfs/io/reader.rs`
- `src/meta/client.rs`
- `src/meta/stores/etcd_watch.rs`
- `src/meta/store.rs`
- `src/meta/stores/database_store.rs`
- `src/daemon/worker.rs`
- `src/daemon/supervisor.rs`

### JuiceFS

- Introduction: <https://juicefs.com/docs/community/introduction/>
- How JuiceFS Works: <https://juicefs.com/docs/community/reference/how_juicefs_works/>
- Cache: <https://juicefs.com/docs/community/guide/cache/>
- Data Processing Workflow: <https://juicefs.com/docs/community/internals/data_processing/>
- Databases for Metadata: <https://juicefs.com/docs/community/administration/databases_for_metadata/>
- Production Deployment Recommendations: <https://juicefs.com/docs/community/administration/production_deployment_recommendations/>
- Status Check and Maintenance: <https://juicefs.com/docs/community/administration/status_check_and_maintenance/>
- Access Control and POSIX ACL: <https://juicefs.com/docs/community/security/posix_acl/>
- Directory Quotas: <https://juicefs.com/docs/community/security/quota/>
- Extended Attributes: <https://juicefs.com/docs/community/guide/xattr/>
- Metadata Backup and Recovery: <https://juicefs.com/docs/community/administration/metadata_dump_load/>
- CSI Introduction: <https://juicefs.com/docs/csi/introduction/>
