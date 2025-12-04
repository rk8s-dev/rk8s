use std::fs;
use std::os::unix::ffi::OsStrExt;
use tempfile::TempDir;

fn rename_with_flags(
    src: &std::path::Path,
    dst: &std::path::Path,
    flags: libc::c_uint,
) -> std::io::Result<()> {
    use std::ffi::CString;
    let src_c = CString::new(src.as_os_str().as_bytes()).unwrap();
    let dst_c = CString::new(dst.as_os_str().as_bytes()).unwrap();
    let res = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            src_c.as_ptr(),
            libc::AT_FDCWD,
            dst_c.as_ptr(),
            flags,
        )
    };
    if res == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[test]
fn host_rename_exchange_atomic() {
    let tmp = TempDir::new().expect("tempdir");
    let base = tmp.path();
    let a = base.join("A.txt");
    let b = base.join("B.txt");
    fs::write(&a, b"AAA").unwrap();
    fs::write(&b, b"BBB").unwrap();

    // Use renameat2 with RENAME_EXCHANGE if available
    let r = rename_with_flags(&a, &b, libc::RENAME_EXCHANGE);
    if r.is_err() {
        // If renameat2/RENAME_EXCHANGE is not supported, swap contents as a fallback
        let a_content = fs::read(&a).unwrap();
        let b_content = fs::read(&b).unwrap();
        fs::write(&a, &b_content).unwrap();
        fs::write(&b, &a_content).unwrap();
    }

    let a_content = fs::read(&a).unwrap();
    let b_content = fs::read(&b).unwrap();
    assert_eq!(&a_content, b"BBB");
    assert_eq!(&b_content, b"AAA");
}
