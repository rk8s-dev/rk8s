//! FUSE/SDK 友好的简化 VFS：基于路径的 create/mkdir/read/write/readdir/stat。

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::BlockStore;
use crate::chuck::util::{split_file_range_into_chunks, ChunkSpan};
use crate::chuck::writer::ChunkWriter;
use crate::meta::{InMemoryMetaStore, MetaStore};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType { File, Dir }

#[derive(Clone, Debug)]
pub struct FileAttr { pub ino: i64, pub size: u64, pub kind: FileType }

#[derive(Clone, Debug)]
pub struct DirEntry { pub name: String, pub ino: i64, pub kind: FileType }

struct VNode { kind: FileType, name: String, parent: Option<i64>, children: HashMap<String, i64> }

impl VNode {
    fn dir(name: String, parent: Option<i64>) -> Self { Self { kind: FileType::Dir, name, parent, children: HashMap::new() } }
    fn file(name: String, parent: Option<i64>) -> Self { Self { kind: FileType::File, name, parent, children: HashMap::new() } }
}

pub struct Fs<S: BlockStore, M: MetaStore> {
    layout: ChunkLayout,
    store: S,
    meta: M,
    base: i64,
    nodes: Mutex<HashMap<i64, VNode>>, // 简单内存命名空间
    lookup: Mutex<HashMap<String, i64>>, // 规范化路径 -> ino
    root: i64,
}

impl<S: BlockStore, M: MetaStore> Fs<S, M> {
    /// 创建 VFS，自动分配根目录 inode。
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Self {
        let mut nodes = HashMap::new();
        let mut lookup = HashMap::new();
        // 分配根目录 inode
        let root_ino = {
            let mut tx = meta.begin().await;
            let ino = tx.alloc_inode().await;
            tx.commit().await.expect("meta commit root");
            ino
        };
        nodes.insert(root_ino, VNode::dir("/".into(), None));
        lookup.insert("/".into(), root_ino);
        // 设定 chunk_id 计算的基数，避免与 chunk 索引冲突（简化实现）。
        let base = 1_000_000_000i64;
        Self { layout, store, meta, base, nodes: Mutex::new(nodes), lookup: Mutex::new(lookup), root: root_ino }
    }

    fn norm_path(p: &str) -> String {
        if p.is_empty() { return "/".into(); }
    let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
        let mut out = String::from("/");
        out.push_str(&parts.join("/"));
        if out.is_empty() { "/".into() } else { out }
    }

    fn split_dir_file(path: &str) -> (String, String) {
        let n = path.rfind('/').unwrap_or(0);
        if n == 0 { ("/".into(), path[1..].into()) } else { (path[..n].into(), path[n + 1..].into()) }
    }

    fn chunk_id_for(&self, ino: i64, chunk_index: u64) -> i64 { ino.checked_mul(self.base).unwrap_or(ino) + chunk_index as i64 }

    /// mkdir -p 风格：创建多级目录。
    pub async fn mkdir_p(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        if &path == "/" { return Ok(self.root); }
    let mut nodes = self.nodes.lock().unwrap();
        let mut lookup = self.lookup.lock().unwrap();
        if let Some(&ino) = lookup.get(&path) { return Ok(ino); }
        // 逐段创建
        let mut cur_ino = self.root;
        let mut cur_path = String::from("/");
        for part in path.trim_start_matches('/').split('/') {
            if part.is_empty() { continue; }
            if cur_path != "/" { cur_path.push('/'); }
            cur_path.push_str(part);
            if let Some(&ino) = lookup.get(&cur_path) {
                cur_ino = ino;
                continue;
            }
            // 新目录 inode
            let ino = {
                let mut tx = self.meta.begin().await;
                let ino = tx.alloc_inode().await;
                tx.commit().await.map_err(|e| e.to_string())?;
                ino
            };
            nodes.insert(ino, VNode::dir(part.to_string(), Some(cur_ino)));
            if let Some(parent) = nodes.get_mut(&cur_ino) { parent.children.insert(part.to_string(), ino); }
            lookup.insert(cur_path.clone(), ino);
            cur_ino = ino;
        }
        Ok(cur_ino)
    }

    /// 创建文件（父目录已存在或通过 mkdir_p 创建）。
    pub async fn create_file(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);
        let dir_ino = self.mkdir_p(&dir).await?;
        let mut nodes = self.nodes.lock().unwrap();
        let mut lookup = self.lookup.lock().unwrap();
        if let Some(d) = nodes.get_mut(&dir_ino) {
            if let Some(&ino) = d.children.get(&name) { return Ok(ino); }
        }
        let ino = {
            let mut tx = self.meta.begin().await;
            let ino = tx.alloc_inode().await;
            tx.commit().await.map_err(|e| e.to_string())?;
            ino
        };
        nodes.insert(ino, VNode::file(name.clone(), Some(dir_ino)));
        if let Some(d) = nodes.get_mut(&dir_ino) { d.children.insert(name.clone(), ino); }
        lookup.insert(path, ino);
        Ok(ino)
    }

    pub async fn stat(&self, path: &str) -> Option<FileAttr> {
        let path = Self::norm_path(path);
        let ino = { self.lookup.lock().unwrap().get(&path).cloned() }?;
        let nodes = self.nodes.lock().unwrap();
        let vnode = nodes.get(&ino)?;
        let kind = vnode.kind;
        let size = self.meta.get_inode_meta(ino).await.map(|m| m.size).unwrap_or(0);
        Some(FileAttr { ino, size, kind })
    }

    pub async fn readdir(&self, path: &str) -> Option<Vec<DirEntry>> {
        let path = Self::norm_path(path);
        let ino = { self.lookup.lock().unwrap().get(&path).cloned() }?;
        let nodes = self.nodes.lock().unwrap();
        let vnode = nodes.get(&ino)?;
        if vnode.kind != FileType::Dir { return None; }
        let mut out = Vec::new();
        for (name, &child_ino) in &vnode.children {
            let child = nodes.get(&child_ino)?;
            out.push(DirEntry { name: name.clone(), ino: child_ino, kind: child.kind });
        }
        Some(out)
    }

    /// 路径是否存在。
    pub fn exists(&self, path: &str) -> bool {
        let path = Self::norm_path(path);
        self.lookup.lock().unwrap().contains_key(&path)
    }

    /// 删除文件（不支持目录）。
    pub async fn unlink(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        let ino = { self.lookup.lock().unwrap().get(&path).cloned() }.ok_or_else(|| "not found".to_string())?;
    let nodes = self.nodes.lock().unwrap();
        let vnode = nodes.get(&ino).ok_or_else(|| "not found".to_string())?;
        if vnode.kind != FileType::File { return Err("is a directory".into()); }
    let parent = vnode.parent.ok_or_else(|| "orphan".to_string())?;
        // 从父目录移除
    drop(nodes);
    if let Some(p) = self.nodes.lock().unwrap().get_mut(&parent) { p.children.retain(|_, v| *v != ino); }
        // 从查找表移除
        self.lookup.lock().unwrap().remove(&path);
        // 删除节点
    self.nodes.lock().unwrap().remove(&ino);
        Ok(())
    }

    /// 删除空目录（不允许删除根）。
    pub async fn rmdir(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        if path == "/" { return Err("cannot remove root".into()); }
        let ino = { self.lookup.lock().unwrap().get(&path).cloned() }.ok_or_else(|| "not found".to_string())?;
    let nodes = self.nodes.lock().unwrap();
        let vnode = nodes.get(&ino).ok_or_else(|| "not found".to_string())?;
        if vnode.kind != FileType::Dir { return Err("not a directory".into()); }
        if !vnode.children.is_empty() { return Err("directory not empty".into()); }
        let parent = vnode.parent.ok_or_else(|| "orphan".to_string())?;
    drop(nodes);
    if let Some(p) = self.nodes.lock().unwrap().get_mut(&parent) { p.children.retain(|_, v| *v != ino); }
        self.lookup.lock().unwrap().remove(&path);
    self.nodes.lock().unwrap().remove(&ino);
        Ok(())
    }

    /// 文件重命名（仅支持文件，目标不得已存在）。
    pub async fn rename_file(&self, old: &str, new: &str) -> Result<(), String> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);
        let (new_dir, new_name) = Self::split_dir_file(&new);
        if self.lookup.lock().unwrap().contains_key(&new) { return Err("target exists".into()); }
        let ino = { self.lookup.lock().unwrap().get(&old).cloned() }.ok_or_else(|| "not found".to_string())?;
        // 创建缺失的父目录并获取其 inode
        self.mkdir_p(&new_dir).await?;
        let new_dir_ino = self.lookup.lock().unwrap().get(&new_dir).cloned().ok_or_else(|| "parent not found".to_string())?;

        // 操作命名空间时小心借用范围，避免同时持有多个可变借用
        let mut nodes = self.nodes.lock().unwrap();
        let old_parent = {
            let vnode = nodes.get(&ino).ok_or_else(|| "not found".to_string())?;
            if vnode.kind != FileType::File { return Err("only file supported".into()); }
            vnode.parent
        };
        // 从旧父目录移除
        if let Some(parent) = old_parent {
            if let Some(p) = nodes.get_mut(&parent) { p.children.retain(|_, v| *v != ino); }
        }
        // 设置新父与名字
        {
            let vnode = nodes.get_mut(&ino).ok_or_else(|| "not found".to_string())?;
            vnode.parent = Some(new_dir_ino);
            vnode.name = new_name.clone();
        }
        if let Some(p) = nodes.get_mut(&new_dir_ino) { p.children.insert(new_name.clone(), ino); }
        drop(nodes);
        // 更新查找表
        let mut lookup = self.lookup.lock().unwrap();
        lookup.remove(&old);
        lookup.insert(new, ino);
        Ok(())
    }

    /// 截断/扩展文件大小（仅更新元数据，数据洞由读路径零填充）。
    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        let path = Self::norm_path(path);
        let ino = { self.lookup.lock().unwrap().get(&path).cloned() }.ok_or_else(|| "not found".to_string())?;
        let mut tx = self.meta.begin().await;
        tx.update_inode_size(ino, size).await.map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())
    }

    /// 写文件（按文件偏移），内部映射到多个 Chunk 写入。
    pub async fn write(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<usize, String> {
        let path = Self::norm_path(path);
        let ino = { self.lookup.lock().unwrap().get(&path).cloned() }.ok_or_else(|| "not found".to_string())?;
        let spans: Vec<ChunkSpan> = split_file_range_into_chunks(self.layout, offset, data.len());
        let mut cursor = 0usize;
        for sp in spans {
            let cid = self.chunk_id_for(ino, sp.chunk_index);
            let mut w = ChunkWriter::new(self.layout, cid, &mut self.store);
            let take = sp.len as usize;
            let buf = &data[cursor..cursor + take];
            let _slice = w.write(sp.offset_in_chunk, buf).await;
            // 记录元数据
            let mut tx = self.meta.begin().await;
            tx.update_inode_size(ino, (offset + cursor as u64 + take as u64) as u64).await.map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;
            cursor += take;
        }
        Ok(data.len())
    }

    /// 读文件（按文件偏移）。
    pub async fn read(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        let path = Self::norm_path(path);
        let ino = { self.lookup.lock().unwrap().get(&path).cloned() }.ok_or_else(|| "not found".to_string())?;
        if len == 0 { return Ok(Vec::new()); }
        let spans: Vec<ChunkSpan> = split_file_range_into_chunks(self.layout, offset, len);
        let mut out = Vec::with_capacity(len);
        for sp in spans {
            let cid = self.chunk_id_for(ino, sp.chunk_index);
            let r = ChunkReader::new(self.layout, cid, &self.store);
            let part = r.read(sp.offset_in_chunk, sp.len as usize).await;
            out.extend(part);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::store::ObjectBlockStore;

    #[tokio::test]
    async fn test_fs_mkdir_create_write_read_readdir() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);
        let meta = InMemoryMetaStore::new();
    let mut fs = Fs::new(layout, store, meta).await;

        fs.mkdir_p("/a/b").await.expect("mkdir_p");
        fs.create_file("/a/b/hello.txt").await.expect("create");
        let data_len = layout.block_size as usize + (layout.block_size / 2) as usize;
        let mut data = vec![0u8; data_len];
        for i in 0..data_len { data[i] = (i % 251) as u8; }
        fs.write("/a/b/hello.txt", (layout.block_size / 2) as u64, &data).await.expect("write");
        let out = fs.read("/a/b/hello.txt", (layout.block_size / 2) as u64, data_len).await.expect("read");
        assert_eq!(out, data);

        let entries = fs.readdir("/a/b").await.expect("readdir");
        assert!(entries.iter().any(|e| e.name == "hello.txt" && e.kind == FileType::File));

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
        let meta = InMemoryMetaStore::new();
    let fs = Fs::new(layout, store, meta).await;

        fs.mkdir_p("/a/b").await.unwrap();
        fs.create_file("/a/b/t.txt").await.unwrap();
        assert!(fs.exists("/a/b/t.txt"));

        // rename file
        fs.rename_file("/a/b/t.txt", "/a/b/u.txt").await.unwrap();
        assert!(!fs.exists("/a/b/t.txt") && fs.exists("/a/b/u.txt"));

        // truncate
        fs.truncate("/a/b/u.txt", (layout.block_size * 2) as u64).await.unwrap();
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
