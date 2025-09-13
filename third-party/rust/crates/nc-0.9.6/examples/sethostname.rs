fn main() {
    let name = "rust-machine";
    let uid = unsafe { nc::getuid() };
    let ret = unsafe { nc::sethostname(name) };
    if uid == 0 {
        assert!(ret.is_ok());
    } else {
        assert!(ret.is_err());
        assert_eq!(ret, Err(nc::EPERM));
    }
}
