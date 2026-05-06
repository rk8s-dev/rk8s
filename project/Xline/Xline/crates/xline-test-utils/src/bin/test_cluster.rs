//! The binary is to quickly setup a cluster for convenient testing
use tokio::signal;
use xline_test_utils::Cluster;

#[tokio::main]
async fn main() {
    // Disable verbose debug logging
    unsafe {
        std::env::set_var("RUST_LOG", "warn");
    }

    println!("=== Starting Xline 3-node Cluster ===");
    let mut cluster = Cluster::new(3).await;
    cluster.start().await;

    println!("\n✓ Cluster is running!");
    println!("\nClient URLs:");
    for (id, addr) in cluster.all_members_client_urls_map() {
        println!("  Server {}: {}", id, addr);
    }

    println!("\n=== Ready for client connections ===");
    println!("You can now run the kv example in another terminal:");
    println!("  cargo run --example kv --package xline-client\n");

    if let Err(e) = signal::ctrl_c().await {
        eprintln!("Unable to listen for shutdown signal: {e}");
    }
}
