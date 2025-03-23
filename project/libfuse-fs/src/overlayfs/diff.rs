

use fuse_backend_rs::api::filesystem::{Context, DirEntry, Entry};
use std::sync::Arc;
use std::{cell::RefCell, io::Result, rc::Rc};
use std::collections::VecDeque;
use crate::overlayfs::layer::Layer;
use crate::passthrough::PassthroughFs;
use crate::util::GPath;

use super::OverlayFs;

   
#[cfg(test)]
mod tests{
    use std::{ffi::CStr, sync::Arc};
    use fuse_backend_rs::{abi::fuse_abi::FsOptions, api::filesystem::Context};
    use crate::passthrough::new_passthroughfs_layer;
    use super::traverse_directory;

    // #[test]
    // fn test_tracerse_drectory(){
    //     let fs = new_passthroughfs_layer("/home/luxian/megatest/lower").await;
    //     fs.init(FsOptions::empty()).unwrap();
    //     let ctx = Context::new();
    //     let afs = Arc::new(fs);
    //     let _out = traverse_directory(&ctx,afs.clone()).unwrap();

    //     let bytes_with_nul: &[u8] = b".\0";
    //     let cstr = CStr::from_bytes_with_nul(bytes_with_nul).expect("CStr creation failed");
    //     let e = afs.lookup(&ctx, afs.root_inode(),cstr).unwrap();
    //     //afs.readdirplus(ctx, inode, handle, size, offset, add_entry);
    //     println!("{:?}",e)
    // }
}