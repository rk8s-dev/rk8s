use anyhow::{Context, Result};
use qlean::{Distro, MachineConfig, create_image, with_machine};

#[tokio::test]
async fn test_with_vm() -> Result<()> {
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 4,
        mem: 4096,
        disk: Some(20),
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async {
            let steps = [
                ("Install dependencies", "sudo apt update && sudo apt install -y git curl pkg-config libssl-dev build-essential"),
                ("Check git", "git --version"),
                ("Clone rk8s repo", "cd /tmp && git clone https://github.com/rk8s-dev/rk8s.git"),
                ("Install Rust", "curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh -s -- -y"),
                ("Check cargo", "source $HOME/.cargo/env && cargo --version"),
                ("Build overlay example", "source $HOME/.cargo/env && cd /tmp/rk8s/project/libfuse-fs && cargo build --example overlay --release"),
                ("Install binary", "sudo install -t /usr/sbin/ -m 700 /tmp/rk8s/project/libfuse-fs/target/release/examples/overlay"),
                ("Set permission", "cd /tmp/rk8s/project/libfuse-fs/tests && chmod +x scripts/xfstests_overlay.sh"),
                ("Run integration tests", "cd /tmp/rk8s/project/libfuse-fs/tests && sudo ./scripts/xfstests_overlay.sh"),
            ];

            for (step_name, cmd) in steps {
                let start = std::time::Instant::now();
                println!("➜ Running step: [{}]", step_name);
                let res = vm.exec(cmd)
                    .await
                    .with_context(|| format!("Step failed: '{}' \nCommand: {}", step_name, cmd))?;
                println!("  ✓ Finished in {:?}", start.elapsed());
                println!("  → STDOUT:\n{}", str::from_utf8(&res.stdout)?.trim());
                println!("  → STDERR:\n{}", str::from_utf8(&res.stderr)?.trim());
            }

            Ok(())
        })
    })
    .await?;

    Ok(())
}
