// use std::{path::Path, sync::Arc, thread::JoinHandle};

// use fuse_backend_rs::{api::{filesystem::FileSystem, server::Server}, transport::{FuseChannel, FuseSession}};
// #[allow(unused)]
// pub struct FuseServer<T: FileSystem + Send + Sync> {
//     pub server: Arc<Server<T>>,
//     pub ch: FuseChannel,
// }
// pub fn run<T: FileSystem + Send + Sync+ 'static>(fuse:Arc<T>,path:&str )->JoinHandle<Result<(), std::io::Error>>{
//     let mut se = FuseSession::new(Path::new(path), "dic", "", false).unwrap();
//     se.mount().unwrap();
//     let ch: FuseChannel = se.new_channel().unwrap();
//     let server = Arc::new(Server::new(fuse));
//     let mut fuse_server = FuseServer { server, ch };
//     // Spawn server thread
//     std::thread::spawn( move || {
//         fuse_server.svc_loop()
//     })

// }
// #[allow(unused)]
// impl <FS:FileSystem+ Send + Sync>FuseServer<FS> {
//     pub fn svc_loop(&mut self) -> Result<(), std::io::Error> {
//         let _ebadf = std::io::Error::from_raw_os_error(libc::EBADF);
//         println!("entering server loop");
//         loop {
//             if let Some((reader, writer)) = self
//                 .ch
//                 .get_request()
//                 .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?
//             {
//                 if let Err(e) = self
//                     .server
//                     .handle_message(reader, writer.into(), None, None)
//                 {
//                     match e {
//                         fuse_backend_rs::Error::EncodeMessage(_ebadf) => {
//                             break;
//                         }
//                         _ => {
//                             print!("Handling fuse message failed");
//                             continue;
//                         }
//                     }
//                 }
//             } else {
//                 print!("fuse server exits");
//                 break;
//             }
//         }
//         Ok(())
//     }
// }

use fuse3::raw::{Filesystem, MountHandle};
use std::ffi::{OsStr, OsString};
use fuse3::{raw::Session, MountOptions};


#[allow(unused)]
pub async fn mount_filesystem<F: Filesystem+ std::marker::Sync + Send +'static >(fs:F,mountpoint:&OsStr) -> MountHandle {
    env_logger::init();
    //let logfs = LoggingFileSystem::new(fs);

    let mount_path: OsString = OsString::from(mountpoint);

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mut mount_options = MountOptions::default();
    // .allow_other(true)
    mount_options
        .force_readdir_plus(true)
        .uid(uid)
        .gid(gid);


    Session::<F>::new(mount_options)
    .mount_with_unprivileged(fs, mount_path)
    .await
    .unwrap()

}
