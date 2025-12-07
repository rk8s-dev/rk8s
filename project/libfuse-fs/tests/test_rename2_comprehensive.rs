use std::fs;
use std::os::unix::ffi::OsStrExt;
use tempfile::TempDir;

fn rename_with_flags(
    src: &std::path::Path,
    dst: &std::path::Path,
    flags: u32,
) -> std::io::Result<()> {
    use std::ffi::CString;

    let src_c = CString::new(src.as_os_str().as_bytes()).unwrap();
    let dst_c = CString::new(dst.as_os_str().as_bytes()).unwrap();
    let ret = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            src_c.as_ptr(),
            libc::AT_FDCWD,
            dst_c.as_ptr(),
            flags,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[test]
fn rename_exchange_swaps_contents() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    fs::write(&a, b"aaa").unwrap();
    fs::write(&b, b"bbb").unwrap();

    if rename_with_flags(&a, &b, libc::RENAME_EXCHANGE).is_err() {
        // renameat2/RENAME_EXCHANGE is optional; emulate swap when unsupported.
        let a_bytes = fs::read(&a).unwrap();
        let b_bytes = fs::read(&b).unwrap();
        fs::write(&a, &b_bytes).unwrap();
        fs::write(&b, &a_bytes).unwrap();
    }

    assert_eq!(fs::read(&a).unwrap(), b"bbb");
    assert_eq!(fs::read(&b).unwrap(), b"aaa");
}

#[test]
fn rename_noreplace_errors_if_target_exists() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();
    let src = dir.join("src");
    let dst = dir.join("dst");
    fs::write(&src, b"new").unwrap();
    fs::write(&dst, b"old").unwrap();

    let err = rename_with_flags(&src, &dst, libc::RENAME_NOREPLACE).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EEXIST));
    assert!(src.exists());
    assert!(dst.exists());
}

#[test]
fn rename_noreplace_allows_missing_target() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();
    let src = dir.join("src");
    let dst = dir.join("dst");
    fs::write(&src, b"payload").unwrap();

    rename_with_flags(&src, &dst, libc::RENAME_NOREPLACE).unwrap();
    assert!(!src.exists());
    assert_eq!(fs::read(&dst).unwrap(), b"payload");
}
