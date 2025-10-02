//! Tests for S3 backend
//!
//! These tests require AWS credentials and a test S3 bucket.
//! Set SLAYERFS_TEST_S3_BUCKET environment variable to run these tests.

#[cfg(test)]
mod tests {
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::s3::{S3Backend, S3Config};
    use std::error::Error;

    async fn get_test_backend() -> Option<S3Backend> {
        let bucket = std::env::var("SLAYERFS_TEST_S3_BUCKET").ok()?;
        let mut config = S3Config::default();
        config.bucket = bucket;
        config.part_size = 1024 * 1024; // 1MB for faster tests
        config.max_concurrency = 2;

        S3Backend::with_config(config).await.ok()
    }

    #[tokio::test]
    #[ignore] // Requires real S3 credentials and bucket
    async fn test_s3_basic_operations() -> Result<(), Box<dyn Error>> {
        let backend = get_test_backend().expect("Set SLAYERFS_TEST_S3_BUCKET env var");
        let client = ObjectClient::new(backend.clone());

        let key = "test-basic-operations";
        let data = b"Hello, S3!";

        // Test put
        client.put_object(key, data).await?;
        println!("✓ Put object");

        // Test get
        let retrieved = client.get_object(key).await?.unwrap();
        assert_eq!(retrieved, data);
        println!("✓ Got object");

        // Test get etag
        let etag = client.get_etag(key).await?;
        assert!(!etag.is_empty());
        println!("✓ Got ETag: {}", etag);

        // Test delete
        client.delete_object(key).await?;
        println!("✓ Deleted object");

        // Verify deletion
        let retrieved = client.get_object(key).await?;
        assert!(retrieved.is_none());
        println!("✓ Confirmed deletion");

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires real S3 credentials and bucket
    async fn test_s3_multipart_upload() -> Result<(), Box<dyn Error>> {
        let backend = get_test_backend().expect("Set SLAYERFS_TEST_S3_BUCKET env var");
        let client = ObjectClient::new(backend.clone());

        let key = "test-multipart-upload";
        // Create data larger than part_size to trigger multipart upload
        let data = vec![42u8; 2 * 1024 * 1024 + 100]; // 2MB + 100 bytes

        // Test put (should use multipart upload)
        client.put_object(key, &data).await?;
        println!("✓ Multipart upload completed");

        // Test get
        let retrieved = client.get_object(key).await?.unwrap();
        assert_eq!(retrieved, data);
        println!("✓ Retrieved multipart data");

        // Cleanup
        client.delete_object(key).await?;
        println!("✓ Cleaned up multipart test data");

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires real S3 credentials and bucket
    async fn test_s3_concurrent_operations() -> Result<(), Box<dyn Error>> {
        let backend = get_test_backend().expect("Set SLAYERFS_TEST_S3_BUCKET env var");
        let client = ObjectClient::new(backend.clone());

        let base_key = "test-concurrent-";
        let data = b"concurrent test data";

        // Test concurrent puts
        let mut put_futures = Vec::new();
        for i in 0..10 {
            let key = format!("{}{}", base_key, i);
            let client_clone = ObjectClient::new(backend.clone());
            let data_clone = data.to_vec();

            put_futures.push(tokio::spawn(async move {
                client_clone.put_object(&key, &data_clone).await
            }));
        }

        for future in put_futures {
            future.await??;
        }
        println!("✓ Concurrent puts completed");

        // Test concurrent gets
        let mut get_futures = Vec::new();
        for i in 0..10 {
            let key = format!("{}{}", base_key, i);
            let client_clone = ObjectClient::new(client.backend.clone());

            get_futures.push(tokio::spawn(async move {
                let result = client_clone.get_object(&key).await?;
                Ok::<_, Box<dyn Error + Send + Sync>>(result)
            }));
        }

        for (i, future) in get_futures.into_iter().enumerate() {
            let retrieved = future.await??;
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap(), data);
            println!("✓ Got concurrent object {}", i);
        }

        // Cleanup
        for i in 0..10 {
            let key = format!("{}{}", base_key, i);
            client.delete_object(&key).await?;
        }
        println!("✓ Cleaned up concurrent test data");

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires real S3 credentials and bucket
    async fn test_s3_error_handling() -> Result<(), Box<dyn Error>> {
        let backend = get_test_backend().expect("Set SLAYERFS_TEST_S3_BUCKET env var");
        let client = ObjectClient::new(backend.clone());

        // Test getting non-existent object
        let result = client.get_object("non-existent-key").await?;
        assert!(result.is_none());
        println!("✓ Non-existent object returns None");

        // Test getting etag of non-existent object
        let result = client.get_etag("non-existent-key").await;
        assert!(result.is_err());
        println!("✓ Non-existent object ETag returns error");

        // Test deleting non-existent object (should not error)
        client.delete_object("non-existent-key").await?;
        println!("✓ Deleting non-existent object succeeds");

        Ok(())
    }
}