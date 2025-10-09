//! FUSE/SDK 友好的简化 VFS：基于路径的 create/mkdir/read/write/readdir/stat。

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::BlockStore;
use crate::chuck::util::{ChunkSpan, split_file_range_into_chunks};
use crate::chuck::writer::ChunkWriter;
use crate::meta::MetaStore;
use crate::meta::entities::content_meta::EntryType;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    File,
    Dir,
}

impl From<EntryType> for FileType {
    fn from(entry_type: EntryType) -> Self {
        match entry_type {
            EntryType::File => FileType::File,
            EntryType::Directory => FileType::Dir,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileAttr {
    pub ino: i64,
    pub size: u64,
    pub kind: FileType,
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub ino: i64,
    pub kind: FileType,
}

/// 命名空间节点（内存）
///
/// 目标与角色：
/// - 这是 Fs 内部用于路径解析与目录树操作的最小“目录项（dentry）/inode 影子”。
/// - 仅承载名称层级关系（父子关系、基名、类型），不保存持久化元数据（大小、时间戳等）。
/// - 持久化信息（如 size、切片映射）交由 `MetaStore` 管理；`VNode` 只负责“能找到这个 inode 吗、它是目录还是文件、它的孩子是谁”。
///
/// 使用与并发：
/// - 通过 `Fs::ns: Mutex<Namespace>` 进行统一并发保护；所有对 `VNode` 的读写都需在持锁状态下进行。
/// - 根节点的 `parent` 为 `None`，其余节点必须有父（除非处于构建中的临时状态）。
/// - 目录节点的 `children` 保存“基名 -> 子 inode”的映射；文件节点的 `children` 为空。
///
/// 限制与简化：
/// - 不包含权限、属主、时间戳、链接计数等；未来如需对接 FUSE/SDK，可在 `MetaStore`/`FileAttr` 扩展。
/// - `name` 为基名（单个路径段），不是完整路径；完整路径由 `Fs::lookup`/`Namespace::lookup` 提供辅助映射。
struct VNode {
    /// 节点类型：文件或目录（影响允许的操作以及 `children` 是否有效）
    kind: FileType,
    /// 该节点在其父目录下的基名（不含斜杠）
    name: String,
    /// 父目录 inode；仅根为 None
    parent: Option<i64>,
    /// 目录子项：基名 -> 子 inode；文件为空
    children: HashMap<String, i64>,
}

impl VNode {
    /// 构造目录节点；`children` 初始为空，由上层在持锁状态下填充。
    fn dir(name: String, parent: Option<i64>) -> Self {
        Self {
            kind: FileType::Dir,
            name,
            parent,
            children: HashMap::new(),
        }
    }
    /// 构造文件节点；`children` 始终保持为空。
    fn file(name: String, parent: Option<i64>) -> Self {
        Self {
            kind: FileType::File,
            name,
            parent,
            children: HashMap::new(),
        }
    }
}

/// 命名空间集合：用单一互斥锁保护节点表与路径索引，避免多把锁的死锁风险。
struct Namespace {
    /// inode -> VNode
    nodes: HashMap<i64, VNode>,
    /// 规范化路径 -> inode（加速路径查询）
    lookup: HashMap<String, i64>,
}

/// 基于路径的简化 VFS（FUSE/SDK 友好）
///
/// 目标概览：
/// - 提供接近 POSIX 的基础文件/目录操作（mkdir_p、create、read、write、readdir、stat 等），便于上层 SDK 与 FUSE 对接。
/// - 命名空间使用内存结构维护（`Namespace`），持久化数据与尺寸由下层 `MetaStore` 与 `BlockStore` 负责。
/// - 按块/按 Chunk 的映射写读，读路径对“洞”进行零填充，写路径按跨度拆分并提交。
///
/// 并发与一致性：
/// - 使用单把互斥锁 `ns: Mutex<Namespace>` 保护目录树与路径索引，避免多锁顺序导致的死锁。
/// - 元数据更新（如 size）通过 `MetaStore` 的事务提交；当前实现将一次 write 的 size 更新聚合为一次提交。
///
/// 约束与注意事项：
/// - 错误返回暂以 `String` 描述，后续建议改为枚举并映射标准 errno。
/// - 不实现权限/时间戳/硬链接等高级语义；`FileAttr` 仅包含 kind 与 size。
/// - unlink/rmdir 目前仅调整命名空间与 size，真实块/切片的 GC 由后续实现负责。
#[allow(unused)]
#[allow(clippy::upper_case_acronyms)]
pub struct VFS<S: BlockStore, M: MetaStore> {
    layout: ChunkLayout,
    store: tokio::sync::Mutex<S>,
    meta: M,
    base: i64,
    ns: Mutex<Namespace>, // 简单内存命名空间（节点+查找表）
    root: i64,
}
#[allow(dead_code)]
impl<S: BlockStore, M: MetaStore> VFS<S, M> {
    /// 公开根 inode（便于 FUSE 处理 `.`/`..` 等）
    pub fn root_ino(&self) -> i64 {
        self.root
    }

    /// 获取给定 inode 的父目录 inode；根的父视为根自身。
    pub fn parent_of(&self, ino: i64) -> Option<i64> {
        let ns = self.ns.lock().unwrap();
        let vnode = ns.nodes.get(&ino)?;
        Some(vnode.parent.unwrap_or(self.root))
    }

    /// 由 inode 还原绝对路径（用于 FUSE open/read 等）
    pub fn path_of(&self, ino: i64) -> Option<String> {
        let ns = self.ns.lock().unwrap();
        self.build_path(&ns, ino)
    }

    /// Build absolute path from inode while namespace is locked
    fn build_path(&self, ns: &Namespace, ino: i64) -> Option<String> {
        let mut cur = ino;
        let mut parts: Vec<String> = Vec::new();
        loop {
            let vnode = ns.nodes.get(&cur)?;
            if vnode.parent.is_none() {
                break;
            }
            parts.push(vnode.name.clone());
            cur = vnode.parent?;
        }
        if parts.is_empty() {
            return Some("/".into());
        }
        parts.reverse();
        let mut out = String::from("/");
        out.push_str(&parts.join("/"));
        Some(out)
    }

    /// 在父目录下按基名查找子项 inode；父必须是目录
    pub fn child_of<'a>(&self, parent: i64, name: impl Into<&'a str>) -> Option<i64> {
        let name = name.into();
        let ns = self.ns.lock().unwrap();
        let p = ns.nodes.get(&parent)?;
        if p.kind != FileType::Dir {
            return None;
        }
        p.children.get(name).cloned()
    }

    /// 按 inode 返回属性（kind 来自命名空间，size 来自 MetaStore）
    pub async fn stat_ino(&self, ino: i64) -> Option<FileAttr> {
        let kind = { self.ns.lock().unwrap().nodes.get(&ino).map(|v| v.kind)? };
        let meta_attr = self.meta.stat(ino).await.ok().flatten()?;
        Some(FileAttr {
            ino,
            size: meta_attr.size,
            kind,
        })
    }

    /// 按 inode 列目录项；非目录或不存在返回 None
    pub async fn readdir_ino(&self, ino: i64) -> Option<Vec<DirEntry>> {
        {
            let ns = self.ns.lock().unwrap();
            if let Some(vnode) = ns.nodes.get(&ino) {
                if vnode.kind != FileType::Dir {
                    return None;
                }

                if !vnode.children.is_empty() {
                    let mut entries = Vec::new();
                    for (name, &child_ino) in &vnode.children {
                        if let Some(child_node) = ns.nodes.get(&child_ino) {
                            entries.push(DirEntry {
                                name: name.clone(),
                                ino: child_ino,
                                kind: child_node.kind,
                            });
                        }
                    }
                    return Some(entries);
                }
            }
        }

        let meta_entries = match self.meta.readdir(ino).await {
            Ok(entries) => entries,
            Err(_) => return None,
        };

        {
            let mut ns = self.ns.lock().unwrap();

            ns.nodes
                .entry(ino)
                .or_insert_with(|| VNode::dir("".to_string(), None));

            if let Some(vnode) = ns.nodes.get_mut(&ino) {
                vnode.children.clear();

                for entry in &meta_entries {
                    vnode.children.insert(entry.name.clone(), entry.ino);
                }
            }

            // Insert child nodes and update path mapping
            for entry in &meta_entries {
                ns.nodes.insert(
                    entry.ino,
                    match entry.kind {
                        FileType::Dir => VNode::dir(entry.name.clone(), Some(ino)),
                        FileType::File => VNode::file(entry.name.clone(), Some(ino)),
                    },
                );

                // Update path lookup to enable path-based operations like write
                if let Some(parent_path) = self.build_path(&ns, ino) {
                    let child_path = if parent_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", parent_path, entry.name)
                    };
                    ns.lookup.insert(child_path, entry.ino);
                }
            }
        }

        let entries: Vec<DirEntry> = meta_entries
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                ino: e.ino,
                kind: e.kind,
            })
            .collect();
        Some(entries)
    }

    /// 创建 VFS，自动分配根目录 inode。
    /// 构造 VFS 实例：
    /// - 分配并注册根目录 inode（/）。
    /// - 初始化内存命名空间（nodes + 路径索引）。
    /// - 约束：根目录的 parent 为 None。
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Result<Self, String> {
        meta.initialize().await.map_err(|e| e.to_string())?;

        let mut nodes = HashMap::new();
        let mut lookup = HashMap::new();

        let root_ino = meta.root_ino();
        nodes.insert(root_ino, VNode::dir("".into(), None)); // 根目录名为空
        lookup.insert("/".into(), root_ino);

        // 设定 chunk_id 计算的基数，避免与 chunk 索引冲突（简化实现）。
        let base = 1_000_000_000i64;
        let ns = Namespace { nodes, lookup };
        let vfs = Self {
            layout,
            store: tokio::sync::Mutex::new(store),
            meta,
            base,
            ns: Mutex::new(ns),
            root: root_ino,
        };

        vfs.load_tree_from_meta().await?;

        Ok(vfs)
    }

    /// Load entire directory tree from MetaStore to rebuild namespace.
    /// Critical for remount scenarios to restore all path mappings.
    /// TODO: Optimize loading for deep directory trees.
    async fn load_tree_from_meta(&self) -> Result<(), String> {
        let mut queue: Vec<(String, i64)> = vec![("/".to_string(), self.root)];

        while let Some((path, ino)) = queue.pop() {
            let entries = match self.meta.readdir(ino).await {
                Ok(entries) => entries,
                Err(e) => {
                    log::warn!("Failed to readdir {} (ino={}): {:?}", path, ino, e);
                    continue;
                }
            };

            {
                let mut ns = self.ns.lock().unwrap();

                ns.nodes
                    .entry(ino)
                    .or_insert_with(|| VNode::dir(path.clone(), None));

                if let Some(vnode) = ns.nodes.get_mut(&ino) {
                    vnode.children.clear();
                    for entry in &entries {
                        vnode.children.insert(entry.name.clone(), entry.ino);
                    }
                }

                for entry in &entries {
                    let child_path = if path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", path, entry.name)
                    };

                    ns.nodes.insert(
                        entry.ino,
                        match entry.kind {
                            FileType::Dir => VNode::dir(entry.name.clone(), Some(ino)),
                            FileType::File => VNode::file(entry.name.clone(), Some(ino)),
                        },
                    );

                    ns.lookup.insert(child_path.clone(), entry.ino);

                    if entry.kind == FileType::Dir {
                        queue.push((child_path, entry.ino));
                    }
                }
            }
        }

        Ok(())
    }

    /// 规范化路径（内部）：
    /// - 去除多余分隔，确保以 `/` 开头；不解析 `.`/`..`（后续可扩展）。
    fn norm_path(p: &str) -> String {
        if p.is_empty() {
            return "/".into();
        }
        let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
        let mut out = String::from("/");
        out.push_str(&parts.join("/"));
        if out.is_empty() { "/".into() } else { out }
    }

    /// 拆分为父目录与基名（内部）。
    fn split_dir_file(path: &str) -> (String, String) {
        let n = path.rfind('/').unwrap_or(0);
        if n == 0 {
            ("/".into(), path[1..].into())
        } else {
            (path[..n].into(), path[n + 1..].into())
        }
    }

    fn chunk_id_for(&self, ino: i64, chunk_index: u64) -> i64 {
        ino.checked_mul(self.base).unwrap_or(ino) + chunk_index as i64
    }

    /// mkdir -p 风格：创建多级目录。
    /// 递归创建目录（mkdir -p）：
    /// - 若部分路径段存在且为“文件”，返回错误 `"not a directory"`。
    /// - 幂等：已存在则返回现有 inode。
    /// - 返回：目标目录的 inode。
    pub async fn mkdir_p(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        if &path == "/" {
            return Ok(self.root);
        }
        if let Some(ino) = { self.ns.lock().unwrap().lookup.get(&path).cloned() } {
            return Ok(ino);
        }
        // 逐段创建
        let mut cur_ino = self.root;
        let mut cur_path = String::from("/");
        for part in path.trim_start_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            if cur_path != "/" {
                cur_path.push('/');
            }
            cur_path.push_str(part);
            if let Some(ino) = { self.ns.lock().unwrap().lookup.get(&cur_path).cloned() } {
                if let Some(v) = { self.ns.lock().unwrap().nodes.get(&ino).map(|v| v.kind) }
                    && v != FileType::Dir
                {
                    return Err("not a directory".into());
                }
                cur_ino = ino;
                continue;
            }
            // 新目录 inode
            let ino = self
                .meta
                .mkdir(cur_ino, part.to_string())
                .await
                .map_err(|e| e.to_string())?;
            {
                let mut ns = self.ns.lock().unwrap();
                ns.nodes
                    .insert(ino, VNode::dir(part.to_string(), Some(cur_ino)));
                if let Some(parent) = ns.nodes.get_mut(&cur_ino) {
                    parent.children.insert(part.to_string(), ino);
                }
                ns.lookup.insert(cur_path.clone(), ino);
            }
            cur_ino = ino;
        }
        Ok(cur_ino)
    }

    /// 创建文件（父目录已存在或通过 mkdir_p 创建）。
    /// 创建普通文件：
    /// - 如父目录不存在，会先行 `mkdir_p`。
    /// - 如同名目录已存在，返回 `"is a directory"`；同名文件存在则返回其 inode。
    /// - 返回：新建或已有文件的 inode。
    pub async fn create_file(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);
        let dir_ino = self.mkdir_p(&dir).await?;
        // 目录必须是目录
        if let Some(kind) = { self.ns.lock().unwrap().nodes.get(&dir_ino).map(|d| d.kind) }
            && kind != FileType::Dir
        {
            return Err("not a directory".into());
        }
        // 冲突：同名目录存在
        if let Some(ino) = {
            self.ns
                .lock()
                .unwrap()
                .nodes
                .get(&dir_ino)
                .and_then(|d| d.children.get(&name))
                .cloned()
        } {
            let kind = {
                self.ns
                    .lock()
                    .unwrap()
                    .nodes
                    .get(&ino)
                    .map(|v| v.kind)
                    .ok_or_else(|| "not found".to_string())?
            };
            return if kind == FileType::Dir {
                Err("is a directory".into())
            } else {
                Ok(ino)
            };
        }
        let ino = self
            .meta
            .create_file(dir_ino, name.clone())
            .await
            .map_err(|e| e.to_string())?;
        {
            let mut ns = self.ns.lock().unwrap();
            ns.nodes
                .insert(ino, VNode::file(name.clone(), Some(dir_ino)));
            if let Some(d) = ns.nodes.get_mut(&dir_ino) {
                d.children.insert(name.clone(), ino);
            }
            ns.lookup.insert(path, ino);
        }
        Ok(ino)
    }

    /// 获取文件属性：
    /// - kind 来源于内存命名空间；size 来源于 `MetaStore`（默认为 0）。
    /// - 未找到返回 None。
    pub async fn stat(&self, path: &str) -> Option<FileAttr> {
        let path = Self::norm_path(path);
        let ino = { self.ns.lock().unwrap().lookup.get(&path).cloned() }?;
        let kind = { self.ns.lock().unwrap().nodes.get(&ino).map(|v| v.kind)? };
        let meta_attr = self.meta.stat(ino).await.ok().flatten()?;
        Some(FileAttr {
            ino,
            size: meta_attr.size,
            kind,
        })
    }

    /// 列举目录：
    /// - 返回目录项列表；路径不存在或非目录返回 None。
    /// - 不包含 "." 与".."（可按需添加）。
    pub async fn readdir(&self, path: &str) -> Option<Vec<DirEntry>> {
        let path = Self::norm_path(path);

        let ino = self.ns.lock().unwrap().lookup.get(&path).cloned()?;

        {
            let ns = self.ns.lock().unwrap();
            if let Some(vnode) = ns.nodes.get(&ino) {
                if vnode.kind != FileType::Dir {
                    return None;
                }

                if !vnode.children.is_empty() {
                    let mut entries = Vec::new();
                    for (name, &child_ino) in &vnode.children {
                        if let Some(child_node) = ns.nodes.get(&child_ino) {
                            entries.push(DirEntry {
                                name: name.clone(),
                                ino: child_ino,
                                kind: child_node.kind,
                            });
                        }
                    }
                    return Some(entries);
                }
            }
        }

        let meta_entries = self.meta.readdir(ino).await.ok()?;

        {
            let mut ns = self.ns.lock().unwrap();

            if let Some(vnode) = ns.nodes.get_mut(&ino) {
                vnode.children.clear();

                for entry in &meta_entries {
                    vnode.children.insert(entry.name.clone(), entry.ino);
                }
            }

            for entry in &meta_entries {
                let child_path = if path == "/" {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", path, entry.name)
                };

                ns.nodes.insert(
                    entry.ino,
                    match entry.kind {
                        FileType::Dir => VNode::dir(entry.name.clone(), Some(ino)),
                        FileType::File => VNode::file(entry.name.clone(), Some(ino)),
                    },
                );
                ns.lookup.insert(child_path, entry.ino);
            }
        }

        let entries: Vec<DirEntry> = meta_entries
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                ino: e.ino,
                kind: e.kind,
            })
            .collect();
        Some(entries)
    }

    /// 路径是否存在。
    /// 路径是否存在（快速查询）。
    pub fn exists(&self, path: &str) -> bool {
        let path = Self::norm_path(path);
        self.ns.lock().unwrap().lookup.contains_key(&path)
    }

    /// 删除文件（不支持目录）。
    /// 删除文件：
    /// - 仅适用于普通文件；若为目录则返回 `"is a directory"`。
    /// - 调整命名空间并移除路径映射；底层数据清理由后续 GC 处理。
    pub async fn unlink(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        let mut ns = self.ns.lock().unwrap();
        let ino = ns
            .lookup
            .get(&path)
            .cloned()
            .ok_or_else(|| "not found".to_string())?;
        let (parent, kind) = {
            let vnode = ns.nodes.get(&ino).ok_or_else(|| "not found".to_string())?;
            (
                vnode.parent.ok_or_else(|| "orphan".to_string())?,
                vnode.kind,
            )
        };
        if kind != FileType::File {
            return Err("is a directory".into());
        }
        if let Some(p) = ns.nodes.get_mut(&parent) {
            p.children.retain(|_, v| *v != ino);
        }
        ns.lookup.remove(&path);
        ns.nodes.remove(&ino);
        Ok(())
    }

    /// 删除空目录（不允许删除根）。
    /// 删除空目录：
    /// - 根目录不可删除；非空返回 `"directory not empty"`。
    pub async fn rmdir(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        if path == "/" {
            return Err("cannot remove root".into());
        }
        let mut ns = self.ns.lock().unwrap();
        let ino = ns
            .lookup
            .get(&path)
            .cloned()
            .ok_or_else(|| "not found".to_string())?;
        let vnode = ns.nodes.get(&ino).ok_or_else(|| "not found".to_string())?;
        if vnode.kind != FileType::Dir {
            return Err("not a directory".into());
        }
        if !vnode.children.is_empty() {
            return Err("directory not empty".into());
        }
        let parent = vnode.parent.ok_or_else(|| "orphan".to_string())?;
        if let Some(p) = ns.nodes.get_mut(&parent) {
            p.children.retain(|_, v| *v != ino);
        }
        ns.lookup.remove(&path);
        ns.nodes.remove(&ino);
        Ok(())
    }

    /// 文件重命名（仅支持文件，目标不得已存在）。
    /// 重命名文件：
    /// - 仅支持文件；目标不得存在；目标父目录若不存在会自动创建。
    /// - 在命名空间锁内进行迁移与路径更新；当前实现不支持覆盖。
    pub async fn rename_file(&self, old: &str, new: &str) -> Result<(), String> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);
        let (new_dir, new_name) = Self::split_dir_file(&new);
        if self.ns.lock().unwrap().lookup.contains_key(&new) {
            return Err("target exists".into());
        }
        let ino = { self.ns.lock().unwrap().lookup.get(&old).cloned() }
            .ok_or_else(|| "not found".to_string())?;
        // 创建缺失的父目录并获取其 inode
        self.mkdir_p(&new_dir).await?;
        let new_dir_ino = self
            .ns
            .lock()
            .unwrap()
            .lookup
            .get(&new_dir)
            .cloned()
            .ok_or_else(|| "parent not found".to_string())?;

        // 操作命名空间时小心借用范围，避免同时持有多个可变借用
        let mut ns = self.ns.lock().unwrap();
        let old_parent = {
            let vnode = ns.nodes.get(&ino).ok_or_else(|| "not found".to_string())?;
            if vnode.kind != FileType::File {
                return Err("only file supported".into());
            }
            vnode.parent
        };
        // 从旧父目录移除
        if let Some(parent) = old_parent
            && let Some(p) = ns.nodes.get_mut(&parent)
        {
            p.children.retain(|_, v| *v != ino);
        }
        // 设置新父与名字
        {
            let vnode = ns
                .nodes
                .get_mut(&ino)
                .ok_or_else(|| "not found".to_string())?;
            vnode.parent = Some(new_dir_ino);
            vnode.name = new_name.clone();
        }
        if let Some(p) = ns.nodes.get_mut(&new_dir_ino) {
            p.children.insert(new_name.clone(), ino);
        }
        // 更新查找表
        ns.lookup.remove(&old);
        ns.lookup.insert(new, ino);
        Ok(())
    }

    /// 截断/扩展文件大小（仅更新元数据，数据洞由读路径零填充）。
    /// 截断/扩展文件大小：
    /// - 仅更新 `MetaStore` 的 size；读路径会对“洞”返回 0 填充。
    /// - 大小收缩不会即时清理块数据（后续可增加惰性 GC）。
    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        let path = Self::norm_path(path);
        let ino = { self.ns.lock().unwrap().lookup.get(&path).cloned() }
            .ok_or_else(|| "not found".to_string())?;
        self.meta
            .set_file_size(ino, size)
            .await
            .map_err(|e| e.to_string())
    }

    /// 写文件（按文件偏移），内部映射到多个 Chunk 写入。
    /// 写入：将文件偏移-长度映射为若干 Chunk 写。
    /// - 分片写入每个相关块；写完后一次性更新 size。
    /// - 返回写入的字节数；当前未保证跨多块的强原子性（后续可引入更细粒度事务）。
    pub async fn write(&self, path: &str, offset: u64, data: &[u8]) -> Result<usize, String> {
        let path = Self::norm_path(path);
        let ino = { self.ns.lock().unwrap().lookup.get(&path).cloned() }
            .ok_or_else(|| "not found".to_string())?;

        let spans: Vec<ChunkSpan> = split_file_range_into_chunks(self.layout, offset, data.len());
        let mut cursor = 0usize;
        for sp in spans {
            let cid = self.chunk_id_for(ino, sp.chunk_index);
            let mut guard = self.store.lock().await;
            let mut w = ChunkWriter::new(self.layout, cid, &mut *guard);
            let take = sp.len;
            let buf = &data[cursor..cursor + take];
            let _slice = w.write(sp.offset_in_chunk, buf).await;
            cursor += take;
        }
        // 一次性更新 size
        let new_size = offset + data.len() as u64;
        self.meta
            .set_file_size(ino, new_size)
            .await
            .map_err(|e| e.to_string())?;
        Ok(data.len())
    }

    /// 读文件（按文件偏移）。
    /// Read by inode directly
    pub async fn read_ino(&self, ino: i64, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let spans: Vec<ChunkSpan> = split_file_range_into_chunks(self.layout, offset, len);
        let mut out = Vec::with_capacity(len);
        for sp in spans {
            let cid = self.chunk_id_for(ino, sp.chunk_index);
            let guard = self.store.lock().await;
            let r = ChunkReader::new(self.layout, cid, &*guard);
            let part = r.read(sp.offset_in_chunk, sp.len).await;
            drop(guard);
            out.extend(part);
        }
        Ok(out)
    }

    /// Read by path (convenience method that uses read_ino internally)
    pub async fn read(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        let path = Self::norm_path(path);
        let ino = { self.ns.lock().unwrap().lookup.get(&path).cloned() }
            .ok_or_else(|| "not found".to_string())?;
        self.read_ino(ino, offset, len).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::store::ObjectBlockStore;
    use crate::meta::create_meta_store_from_url;

    #[tokio::test]
    async fn test_fs_mkdir_create_write_read_readdir() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);

        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let fs = VFS::new(layout, store, meta).await.unwrap();

        fs.mkdir_p("/a/b").await.expect("mkdir_p");
        fs.create_file("/a/b/hello.txt").await.expect("create");
        let data_len = layout.block_size as usize + (layout.block_size / 2) as usize;
        let mut data = vec![0u8; data_len];
        for (i, b) in data.iter_mut().enumerate().take(data_len) {
            *b = (i % 251) as u8;
        }
        fs.write("/a/b/hello.txt", (layout.block_size / 2) as u64, &data)
            .await
            .expect("write");
        let out = fs
            .read("/a/b/hello.txt", (layout.block_size / 2) as u64, data_len)
            .await
            .expect("read");
        assert_eq!(out, data);

        let entries = fs.readdir("/a/b").await.expect("readdir");
        assert!(
            entries
                .iter()
                .any(|e| e.name == "hello.txt" && e.kind == FileType::File)
        );

        let stat = fs.stat("/a/b/hello.txt").await.unwrap();
        assert_eq!(stat.kind, FileType::File);
        assert!(stat.size >= data_len as u64);
    }

    #[tokio::test]
    async fn test_fs_unlink_rmdir_rename_truncate() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);

        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let fs = VFS::new(layout, store, meta).await.unwrap();

        fs.mkdir_p("/a/b").await.unwrap();
        fs.create_file("/a/b/t.txt").await.unwrap();
        assert!(fs.exists("/a/b/t.txt"));

        // rename file
        fs.rename_file("/a/b/t.txt", "/a/b/u.txt").await.unwrap();
        assert!(!fs.exists("/a/b/t.txt") && fs.exists("/a/b/u.txt"));

        // truncate
        fs.truncate("/a/b/u.txt", layout.block_size as u64 * 2)
            .await
            .unwrap();
        let st = fs.stat("/a/b/u.txt").await.unwrap();
        assert!(st.size >= (layout.block_size * 2) as u64);

        // unlink and rmdir
        fs.unlink("/a/b/u.txt").await.unwrap();
        assert!(!fs.exists("/a/b/u.txt"));
        // dir empty then rmdir
        fs.rmdir("/a/b").await.unwrap();
        assert!(!fs.exists("/a/b"));
    }
}
