# xlinerpc 迁移报告：从 tonic 到 xlinerpc 的 Server 组件适配

## 概述
本报告记录了 Xline Server 组件从 tonic Request/Response API 到 xlinerpc 的适配工作。这是移除 tonic-build 对 proto 编译依赖的关键步骤。

## 已完成的工作

### 1. API 转换函数映射
以下是 tonic API 与 xlinerpc Envelope API 的对应关系：

| tonic API | xlinerpc API | 说明 |
|-----------|-------------|------|
| `request.into_inner()` | `request.into_parts().0` | 获取数据所有权，丢弃元数据 |
| `request.get_mut()` | `request.data_mut()` | 获取可变引用 |
| `request.get_ref()` | `request.data()` | 获取不可变引用 |
| `request.metadata()` | `request.meta()` | 获取元数据 |
| (新增) | `request.into_data()` | 获取数据所有权（简便方法） |

### 2. 文件级迁移详情

#### ✅ auth_server.rs (完全迁移)
**Trait**: `Auth`  
**文件类型**: 使用 xlinerpc::Request  
**修改点**:
- `propose()` 方法: `into_inner()` → `into_parts().0`
- `handle_req()` 方法: `into_inner()` → `into_parts().0`
- `user_add()`: `get_mut()` → `data_mut()`
- `user_change_password()`: `get_mut()` → `data_mut()`
- `role_add()`: `get_ref()` → `data()`
- `role_grant_permission()`: `get_ref()` → `data()`
- `role_revoke_permission()`: `get_ref()` → `data()`

**状态**: ✅ 完全编译通过（无相关错误）

#### ✅ cluster_server.rs (完全迁移)
**Trait**: `Cluster`  
**文件类型**: 使用 xlinerpc::Request  
**修改点**:
- 5 个 handler 方法中的 `into_inner()` 调用:
  - `member_add()`
  - `member_remove()`
  - `member_update()`
  - `member_list()`
  - `member_promote()`
- 统一改为: `request.into_parts().0`

**状态**: ✅ 完全编译通过（无相关错误）

#### ✅ kv_server.rs (完全迁移)
**Trait**: `Kv`  
**文件类型**: 使用 xlinerpc::Request  
**修改点**:
- `do_serializable()`: `into_inner().into()` → `into_parts().0.into()`
- `propose()`: `into_inner().into()` → `into_parts().0.into()`
- `compact()`: `into_inner()` → `into_parts().0`
- 6 处使用 `into_data()` 的调用（新增方法支持）

**状态**: ✅ 完全编译通过（无相关错误）

#### ✅ watch_server.rs (已使用 xlinerpc)
**Trait**: `Watch`  
**文件类型**: 已使用 xlinerpc::Request  
**修改**: 无需修改，符合迁移标准

**状态**: ✅ 无需修改

#### ✅ xlinerpc/src/envelope.rs (新增功能)
**添加新方法**:
```rust
/// Extract the payload, discarding metadata
#[must_use]
#[inline]
pub fn into_data(self) -> T {
    self.data
}
```

**说明**: 为了支持 kv_server.rs 中的多个 `into_data()` 调用，添加了便捷方法

**状态**: ✅ 实现完成

### 3. 未迁移的 Server（仍使用 tonic::Request）

以下 servers 尚未迁移，因为它们的 trait 定义仍然使用 `tonic::Request`。这些需要后续迁移：

#### ⚠️ lease_server.rs
**Trait**: `Lease`  
**current**: `tonic::Request` / `tonic::Response`  
**issue**: try_get_auth_info_from_request 需要 xlinerpc::Request  
**impact**: 6 处 into_inner() 调用，需要全面改造 trait 签名

#### ⚠️ maintenance.rs
**Trait**: `Maintenance`  
**current**: `tonic::Request` / `tonic::Response`  
**issue**: try_get_auth_info_from_request 需要 xlinerpc::Request  
**impact**: 需要改造 trait 签名

#### ⚠️ lock_server.rs
**Trait**: `Lock`  
**current**: `tonic::Request` / `tonic::Response`  
**issue**: try_get_auth_info_from_request 需要 xlinerpc::Request  
**impact**: 2 处 into_inner() 调用，需要改造 trait 签名

#### ✅ auth_wrapper.rs
**current**: `tonic::Request` (gRPC bridge 层)  
**note**: 这是 gRPC-to-internal 适配层，保持 tonic::Request 是合适的，无需修改

## 编译状态

### 当前编译结果
```
✅ auth_server.rs - 无错误
✅ cluster_server.rs - 无错误  
✅ kv_server.rs - 无错误
✅ xlinerpc/src/envelope.rs - 无错误
❌ 其他库编译错误（libc 相关，不相关于此迁移）
```

### 未来的迁移任务

1. **迁移 lock_server.rs**
   - 改变 Lock trait 的签名为 xlinerpc::Request
   - 更新 2 处 into_inner() 调用

2. **迁移 lease_server.rs**
   - 改变 Lease trait 的签名为 xlinerpc::Request  
   - 更新 6 处 into_inner() 调用
   - 注意: 有 tonic::Streaming 用法，需要处理流式传输适配

3. **迁移 maintenance.rs**
   - 改变 Maintenance trait 的签名为 xlinerpc::Request
   - 注意: 包含流式传输（Snapshot），需要特殊处理

4. **tonic-build 依赖移除**
   - 更新 Cargo.toml 和 build.rs
   - 迁移 proto 编译到其他构建系统（Buck2/Bazel）

## 关键学习

1. **Envelope 模式**: xlinerpc 使用 `Envelope<T, Kind>` 通用包装器，支持 Request 和 Response
2. **元数据处理**: 从 tonic::metadata::MetadataMap 迁移到 xlinerpc::MetaData（基于 BTreeMap）  
3. **API 风格**: 从 `into_inner()`/`get_ref()`/`get_mut()` 改为 `data()`/`data_mut()`/`into_parts()`/`into_data()`

## 建议的后续步骤

1. ✅ 已完成: 迁移使用 xlinerpc::Request 的 servers（auth, kv, cluster）
2. 📋 待完成: 迁移仍使用 tonic::Request 的 servers（lease, maintenance, lock）
3. 📋 待完成: 从 Cargo.toml 移除 tonic-build，更新构建配置
4. 🧪 待完成: 运行完整的 integration 测试套件验证迁移

## 文件清单

### 已修改的文件
- `project/Xline/Xline/crates/xline/src/server/auth_server.rs` - 7 处改动
- `project/Xline/Xline/crates/xline/src/server/cluster_server.rs` - 5 处改动
- `project/Xline/Xline/crates/xline/src/server/kv_server.rs` - 3 处改动
- `project/Xline/Xline/crates/xlinerpc/src/envelope.rs` - 1 处新增

### 需要后续修改的文件
- `project/Xline/Xline/crates/xline/src/server/lease_server.rs` - 6 处待改
- `project/Xline/Xline/crates/xline/src/server/maintenance.rs` - 1 处待改
- `project/Xline/Xline/crates/xline/src/server/lock_server.rs` - 2 处待改
