
# nc

![Build status](https://github.com/xushaohua/nc/actions/workflows/rust.yml/badge.svg)
[![Latest version](https://img.shields.io/crates/v/nc.svg)](https://crates.io/crates/nc)
[![Documentation](https://docs.rs/nc/badge.svg)](https://docs.rs/nc)
![Minimum rustc version](https://img.shields.io/badge/rustc-1.63+-yellow.svg)
![License](https://img.shields.io/crates/l/nc.svg)

Access system calls directly without `std` or `libc`.

- [Documentation](https://docs.rs/nc)
- [Release notes](https://github.com/xushaohua/nc/tags)

Features:
- No glibc or musl required
- Access syscalls directly, via assembly
- No global errno variable, every function returns an errno instead
- Support latest kernel APIs, like io-uring and pidfd, introduced in linux 5.0+

## Usage

Add this to `Cargo.toml`:
```toml
[dependencies]
nc = "0.9"
```

## Examples

Get file stat:
```rust
let mut statbuf = nc::stat_t::default();
match unsafe { nc::stat("/etc/passwd", &mut statbuf) } {
    Ok(_) => println!("s: {:?}", statbuf),
    Err(errno) => eprintln!("Failed to get file status, got errno: {}", errno),
}
```

Get human-readable error string:
```rust
let errno = nc::EPERM;
println!("err: {:?}", nc::strerror(errno);
```

Fork process:
```rust
let pid = unsafe { nc::fork() };
match pid {
    Err(errno) => eprintln!("Failed to call fork(), err: {}", nc::strerror(errno)),
    Ok(0) => {
        // Child process
        println!("[child] pid: {}", unsafe { nc::getpid() });
        let args = ["ls", "-l", "-a"];
        let env = ["DISPLAY=wayland"];
        let ret = unsafe { nc::execve("/bin/ls", &args, &env) };
        assert!(ret.is_ok());
    }
    Ok(child_pid) => {
        // Parent process
        println!("[main] child pid is: {child_pid}");
    }
}
```

Kill self:
```rust
let pid = unsafe { nc::getpid() };
let ret = unsafe { nc::kill(pid, nc::SIGTERM) };
// Never reach here.
println!("ret: {:?}", ret);
```

Or handle signals:
```rust
fn handle_alarm(signum: i32) {
    assert_eq!(signum, nc::SIGALRM);
}

fn main() {
    let sa = nc::new_sigaction(handle_alarm);
    let ret = unsafe { nc::rt_sigaction(nc::SIGALRM, Some(&sa), None) };
    assert!(ret.is_ok());
    let remaining = unsafe { nc::alarm(1) };
    let mask = nc::sigset_t::default();
    let ret = unsafe { nc::rt_sigsuspend(&mask) };
    assert!(ret.is_err());
    assert_eq!(ret, Err(nc::EINTR));
    assert_eq!(remaining, Ok(0));
}
```

## Supported Operating Systems and Architectures

- linux
  - aarch64
  - arm
  - loongarch64
  - mips
  - mips64
  - mips64el
  - mipsel
  - powerpc64
  - powerpc64el
  - riscv64
  - s390x
  - x86
  - x86-64
- android
  - aarch64
- freebsd
  - x86-64
- netbsd
  - x86-64
- mac os
  - x86-64

## Related projects

- [nix](https://github.com/nix-rust/nix)
- [relibc](https://gitlab.redox-os.org/redox-os/relibc.git)
- [Linux Syscall Support](https://chromium.googlesource.com/linux-syscall-support)
- [syscall pkg in golang](https://github.com/golang/go/tree/master/src/syscall)

## License

This library is govered by [Apache-2.0 License](LICENSE).
