// Library crate for SlayerFS: re-export internal modules for reuse by examples and external bins.
// NOTE: 当前仓库包含若干占位/预留结构与函数，短期内可能未在所有构建目标中使用。
// 为减少 CI 噪音，在开发阶段允许 dead_code；待接口稳定后可逐步移除或改为 feature gate。
#![allow(dead_code)]

pub mod cadapter;
pub mod chuck;
pub mod daemon;
pub mod fuse;
pub mod meta;
pub mod vfs;
