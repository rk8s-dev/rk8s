// Copyright (C) 2023 Ant Group. All rights reserved.
//  2024 From [fuse_backend_rs](https://github.com/cloud-hypervisor/fuse-backend-rs)
// SPDX-License-Identifier: Apache-2.0
use rfuse3::FileType;

pub(super) fn is_dir(st: &FileType) -> bool {
    *st == FileType::Directory
}
