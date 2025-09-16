// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    let name = "nc-mq-notify";
    let ret = unsafe {
        nc::mq_open(
            name,
            nc::O_CREAT | nc::O_RDWR | nc::O_EXCL,
            (nc::S_IRUSR | nc::S_IWUSR) as nc::umode_t,
            None,
        )
    };
    assert!(ret.is_ok());
    let mq_id = ret.unwrap();

    let mut attr = nc::mq_attr_t::default();
    let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
    assert!(ret.is_ok());
    println!("attr: {:?}", attr);

    let msg = "Hello, Rust";
    let prio = 42;
    let timeout = nc::timespec_t {
        tv_sec: 1,
        tv_nsec: 0,
    };
    let ret = unsafe { nc::mq_timedsend(mq_id, msg.as_bytes(), prio, &timeout) };
    assert!(ret.is_ok());

    let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
    assert!(ret.is_ok());
    assert_eq!(attr.mq_curmsgs, 1);

    let mut buf = vec![0_u8; attr.mq_msgsize as usize];
    let mut recv_prio = 0;
    let read_timeout = nc::timespec_t {
        tv_sec: 1,
        tv_nsec: 0,
    };
    let ret = unsafe { nc::mq_timedreceive(mq_id, &mut buf, &mut recv_prio, &read_timeout) };
    if let Err(errno) = ret {
        eprintln!("mq_timedreceive() error: {}", nc::strerror(errno));
    }
    assert!(ret.is_ok());
    let n_read = ret.unwrap() as usize;
    assert_eq!(n_read, msg.len());

    let ret = unsafe { nc::close(mq_id) };
    assert!(ret.is_ok());
    let ret = unsafe { nc::mq_unlink(name) };
    assert!(ret.is_ok());
}
