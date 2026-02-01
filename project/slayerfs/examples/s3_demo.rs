// S3 Demo program: demonstrate S3 backend usage
//
// This demo shows how to use SlayerFS with an S3 backend.
// Make sure you have AWS credentials configured (via environment variables, IAM roles, or ~/.aws/credentials)

use slayerfs::{
    ChunkLayout, ObjectBlockStore, ObjectClient, S3Backend, S3Config, VfsClient,
    create_meta_store_from_url,
};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logger
    env_logger::init();

    // Get S3 bucket from command line arguments or environment
    let args: Vec<String> = std::env::args().collect();
    let bucket = if args.len() > 1 {
        args[1].clone()
    } else {
        std::env::var("SLAYERFS_S3_BUCKET").unwrap_or_else(|_| "slayerfs-demo-bucket".to_string())
    };

    println!("Using S3 bucket: {}", bucket);

    // Configure S3 backend
    let config = S3Config {
        bucket: bucket.clone(),
        region: std::env::var("SLAYERFS_S3_REGION").ok(),
        part_size: 16 * 1024 * 1024,
        max_concurrency: 8,
        ..Default::default()
    };

    // Create S3 backend
    println!("Initializing S3 backend...");
    let s3_backend = S3Backend::with_config(config)
        .await
        .map_err(|e| format!("Failed to create S3 backend: {:?}", e))?;

    // Create object client
    let object_client = ObjectClient::new(s3_backend);

    // Create chunk layout (default 64MB chunks, 4MB blocks)
    let layout = ChunkLayout::default();

    // Create memory metadata store (for demo purposes)
    let meta_handle = create_meta_store_from_url("sqlite::memory:")
        .await
        .expect("create meta store");

    // Create SDK client (FileSystem-backed)
    let store = ObjectBlockStore::new(object_client);
    let meta_store = meta_handle.store();
    let client = VfsClient::new(layout, store, meta_store)
        .await
        .expect("create filesystem client");

    // Test basic operations
    println!("Testing basic S3 operations...");

    // Create a directory
    let dir_path = "/demo-s3";
    client.mkdir_p(dir_path).await?;
    println!("✓ Created directory: {}", dir_path);

    // Create a file
    let file_path = "/demo-s3/test.txt";
    client.create_file(file_path, false).await?;
    println!("✓ Created file: {}", file_path);

    // Write some data
    let test_data = b"Hello, SlayerFS S3 Backend! This is test data stored in S3.";
    client.write_at(file_path, 0, test_data).await?;
    println!("✓ Wrote {} bytes to file", test_data.len());

    // Read the data back
    let read_data = client.read_at(file_path, 0, test_data.len()).await?;
    assert_eq!(read_data, test_data);
    println!("✓ Read {} bytes back from file", read_data.len());

    // Test larger data (crossing block boundaries)
    let large_data = vec![42u8; layout.block_size as usize + 1000];
    client
        .write_at(file_path, layout.block_size as u64, &large_data)
        .await?;
    println!(
        "✓ Wrote large data ({}) bytes starting at offset {}",
        large_data.len(),
        layout.block_size
    );

    // Read back the large data
    let read_large = client
        .read_at(file_path, layout.block_size as u64, large_data.len())
        .await?;
    assert_eq!(read_large, large_data);
    println!("✓ Read large data back successfully");

    // Test file metadata
    let metadata = client.stat(file_path).await?;
    println!(
        "✓ File metadata: size={}, kind={:?}",
        metadata.size, metadata.kind
    );

    // List directory contents
    let entries = client.readdir(dir_path).await?;
    println!(
        "✓ Directory {} contains {} entries:",
        dir_path,
        entries.len()
    );
    for entry in &entries {
        println!(
            "  - {} ({})",
            entry.name,
            match entry.kind {
                slayerfs::VfsFileType::Dir => "directory",
                slayerfs::VfsFileType::File => "file",
                slayerfs::VfsFileType::Symlink => "symlink",
            }
        );
    }

    // Test delete functionality
    client.unlink(file_path).await?;
    println!("✓ Deleted file: {}", file_path);

    // Verify deletion
    match client.stat(file_path).await {
        Err(_) => println!("✓ Confirmed file deletion"),
        Ok(_) => return Err("File should have been deleted".into()),
    }

    println!("\n🎉 All S3 backend tests passed!");
    println!("S3 backend is working correctly with bucket '{}'", bucket);

    Ok(())
}
