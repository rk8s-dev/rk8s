# rust-cni 
[![CI](https://github.com/jokemanfire/rust-cni/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jokemanfire/rust-cni/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/l/containerd-client)](https://github.com/jokemanfire/rust-cni/blob/main/LICENSE)

This is the CNI plugin impl by rust for container-runtime create CNI network.



## requirements
* Install cni plugin in /opt/cni/bin
* Prepare cni config in /etc/cni/net.d


## Run test
run it as root.
```bash
cargo test --test it_test --  --test-threads=1 --nocapture
```

## example

```Rust
use std::{fs::File, io};
use rust_cni::cni::Libcni;
use netns_rs::NetNs;
use nix::sched::setns;

fn create_ns() -> Result<NetNs, String> {
    let ns = NetNs::new("ns_name").unwrap();
    println!("{:?}", ns.path());
    Ok(ns)
}

fn main() {
    let ns = create_ns().unwrap();
    // Default cni config is in /etc/cni/net.d/
    // Default cni bin is in /opt/cni/bin/  
    let mut cni = Libcni::default();
    cni.load_default_conf();
    cni.add_lo_network().unwrap();

    let id = "test".to_string();
    let path = ns.path().to_string_lossy().to_string();
    cni.setup(id.clone(), path.clone()).unwrap();

    println!("try to remove --------------------");
    cni.remove(id.clone(), path.clone()).unwrap();
    ns.remove().unwrap();
}
```

## License
This project is licensed under the Apache License 2.0. See the LICENSE file for details.

## references
* Containerd cni plugins （https://github.com/containerd/go-cni）
* cni-rs (https://github.com/divinerapier/cni-rs)


## Contributing
Contributions are welcome! Please open an issue or submit a pull request if you have any improvements or bug fixes.

For more detailed information, please refer to the source code and documentation.