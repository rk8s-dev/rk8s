/// Create an endpoint for communication.
pub unsafe fn __socket30(domain: i32, sock_type: i32, protocol: i32) -> Result<i32, Errno> {
    let domain = domain as usize;
    let sock_type = sock_type as usize;
    let protocol = protocol as usize;
    syscall3(SYS___SOCKET30, domain, sock_type, protocol).map(|ret| ret as i32)
}
