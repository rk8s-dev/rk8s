# SlayerFS rename操作设计文档

## 架构概述

SlayerFS 在三个层次实现重命名操作：

1. **VFS 层** (`vfs/fs.rs`)：基于路径的高级 API，包含验证和协调功能
2. **MetaClient 层** (`meta/client.rs`)：智能缓存和元数据管理
3. **MetaStore 层** (`meta/stores/`)：持久化存储后端（数据库、etcd、Redis）

## 核心语义

### POSIX 兼容性

SlayerFS 重命名操作遵循 POSIX rename(2) 语义：

- **原子性**：重命名操作在元数据级别是原子的
- **替换规则**：
  - 文件/目录可以替换文件/符号链接
  - 目录只能替换空目录
  - 不能替换非空目录
- **跨目录**：支持在目录间移动
- **同目录**：针对同目录重命名的优化快速路径

### 扩展功能

除了 POSIX 兼容性外，SlayerFS 还提供：

- **重命名标志**：支持 RENAME_NOREPLACE、RENAME_EXCHANGE（RENAME_WHITEOUT 预留接口但未实现）
- **批处理操作**：高效处理多个重命名操作
- **循环检测**：防止将目录重命名到其自身的子目录中
- **原子交换**：RENAME_EXCHANGE 通过数据库事务保证原子性

## 实现细节

### VFS 层 (`vfs/fs.rs`)

#### 核心方法

```rust
pub async fn rename(&self, old: &str, new: &str) -> Result<(), VfsError>
```

**算法**：

1. 规范化路径
2. 检测同目录优化
3. 验证操作前提条件
4. 处理目标替换逻辑
5. 执行元数据重命名
6. 更新缓存

#### 快速路径优化

```rust
fn rename_same_directory(&self, dir: &str, old_name: &str, new_name: &str)
```

**优势**：

- 避免重复解析父目录
- 减少 ~30-50% 的路径解析开销
- 保持相同的验证逻辑

#### 扩展 API 方法

**已实现**：

```rust
pub async fn rename_with_flags(&self, old: &str, new: &str, flags: RenameFlags)
pub async fn rename_noreplace(&self, old: &str, new: &str)
pub async fn rename_exchange(&self, old: &str, new: &str)
pub async fn can_rename(&self, old: &str, new: &str) -> Result<(), VfsError>
pub async fn rename_batch(&self, operations: Vec<(String, String)>) -> Vec<Result<(), VfsError>>
```

**已移除**（调试功能）：

- `rename_dry_run()` - 仅用于调试，已从生产代码中移除
- `rename_with_parents()` - 内部优化，已合并到主 rename 逻辑

### MetaClient 层 (`meta/client.rs`)

#### 缓存策略

**预验证**：

```rust
async fn validate_rename_operation(&self, old_parent: i64, old_name: &str, new_parent: i64, new_name: &str)
```

**缓存更新**：

```rust
async fn update_cache_after_rename(&self, old_parent: i64, old_name: &str, new_parent: i64, new_name: &str, src_ino: i64, src_attr: &Option<FileAttr>)
```

**缓存失效规则**：

- 使旧目录和新目录路径缓存失效
- 使父目录状态缓存失效（mtime/ctime 已更改）
- 为硬链接更新 inode 到父目录的关系
- 预加载常用缓存条目

#### 一致性管理

- **操作前状态**：在修改前捕获缓存状态
- **原子更新**：确保所有缓存更新成功或回滚
- **错误恢复**：在失败时尝试恢复缓存一致性

### MetaStore 层

#### 数据库实现 (`database_store.rs`)

**事务逻辑**：

```sql
-- 原子重命名事务
BEGIN;
-- 验证源存在并获取属性
-- 检查目标替换规则
-- 更新 content_meta 条目
-- 为单链接更新 file_meta.parent 字段
-- 为多链接更新 link_parent_meta
-- 更新目录 mtime/ctime
COMMIT;
```

**硬链接处理**：

- `nlink = 1`：直接更新 `file_meta.parent`
- `nlink > 1`：管理 `link_parent_meta` 条目

### FUSE 层集成 (`fuse/mod.rs`)

#### 错误映射

```rust
match vfs_error {
    VfsError::NotFound { .. } => libc::ENOENT,
    VfsError::AlreadyExists { .. } => libc::EEXIST,
    VfsError::CircularRename { .. } => libc::EINVAL,
    VfsError::InvalidRenameTarget { .. } => libc::EINVAL,
    // ... 其他映射
}
```

## 错误处理

### 错误类型

- `CircularRename`：防止目录移动到自己的子目录中
- `InvalidRenameTarget`：无效的名称或路径
- `DirectoryNotEmpty`：不能替换非空目录
- `NotADirectory`：类型不匹配错误

### 验证阶段

1. **输入验证**：检查路径格式和字符（空名称、'/'、'\0'）
2. **存在性检查**：验证源存在，验证父目录
3. **类型兼容性**：检查源/目标类型规则
4. **循环引用检测**：防止目录循环（基于 inode 的祖先链遍历）
5. **原子性保证**：通过数据库事务确保操作的原子性

## 性能优化

### 同目录重命名的快速路径

- **检测**：比较规范化的目录路径
- **优势**：消除重复的父目录解析
- **实现**：`rename_same_directory()` 方法

### 批处理操作

- **顺序处理**：对大多数用例简单有效
- **错误隔离**：单个操作失败不影响其他操作
- **内存效率**：操作协调的最小开销

### 缓存预加载

- **预测性加载**：预加载重命名的条目和受影响的目录
- **减少延迟**：后续操作找到热缓存
- **内存管理**：智能缓存大小管理

## 测试策略

### 单元测试

**边界条件**（已实现）：

- 同目录重命名
- 跨目录移动
- 不存在的源路径
- 现有目标处理
- 目录替换规则
- 硬链接重命名场景
- 原子交换操作（RENAME_EXCHANGE）
- 创建时间保留验证

**错误情况**（已实现）：

- 循环重命名尝试（增强的基于 inode 的检测）
- 无效路径格式（空名称、包含 '/' 或 '\0'）
- 缺失条目（源或目标不存在）
- 存储后端失败处理

### 集成测试

**FUSE 层验证**：

- 通过 FUSE 接口的端到端重命名操作
- 错误代码转换准确性
- 并发操作处理

### 并发测试

**多线程场景**：

- 同目录中的并行重命名
- 跨目录并发操作
- 争用下的缓存一致性
- 锁争用模式

### 故障注入测试

**错误恢复验证**：

- 重命名期间存储后端失败
- 缓存更新失败
- 部分操作回滚
- 系统资源耗尽

## 安全考虑

### 路径遍历保护

- **规范化**：所有路径在处理前都被规范化
- **验证**：拒绝包含 `..` 或绝对组件的路径
- **循环检测**：防止目录循环

### 资源限制

- **名称长度**：文件名 255 字符限制
- **路径深度**：防止极深的目录层次结构
- **操作批处理**：批处理操作大小的合理限制

## 未来增强

### 高级功能

- **重命名标志**：完整的 RENAME_EXCHANGE 实现
- **写时复制**：大文件重命名的延迟复制
- **分布式协调**：跨节点重命名操作
- **配额集成**：遵守存储配额的重命名操作

### 性能改进

- **并行批处理**：独立重命名的并发执行
- **内存映射元数据**：减少大型目录的数据库压力
- **预测性预取**：基于机器学习的缓存预热
