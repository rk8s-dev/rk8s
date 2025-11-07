//! S3 adapter: simplified aws-sdk-s3 implementation with multipart upload, retries, and validation.

use crate::cadapter::client::ObjectBackend;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::primitives::SdkBody;
use aws_sdk_s3::{Client, config::Region};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use md5;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

/// S3 backend configuration options
#[derive(Debug, Clone)]
pub struct S3Config {
    /// S3 bucket name
    pub bucket: String,
    /// AWS region (optional, will use default if not specified)
    pub region: Option<String>,
    /// Part size for multipart uploads in bytes (default: 8MB)
    pub part_size: usize,
    /// Maximum concurrent multipart upload parts (default: 4)
    pub max_concurrency: usize,
    /// Maximum retry attempts for failed operations (default: 3)
    pub max_retries: u32,
    /// Base delay for exponential backoff in milliseconds (default: 100ms)
    pub retry_base_delay: u64,
    /// Enable MD5 checksums for uploads (default: true)
    pub enable_md5: bool,
    /// Custom endpoint URL (e.g. for MinIO or localstack)
    pub endpoint: Option<String>,
    /// Force path-style access (required for some S3-compatible services)
    pub force_path_style: bool,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            region: None,
            part_size: 8 * 1024 * 1024, // 8MB
            max_concurrency: 4,
            max_retries: 3,
            retry_base_delay: 100,
            enable_md5: true,
            endpoint: None,
            force_path_style: false,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct S3Backend {
    client: Client,
    config: S3Config,
}

#[allow(dead_code)]
impl S3Backend {
    /// Create new S3 backend with default configuration
    pub async fn new(bucket: impl Into<String>) -> Result<Self> {
        let config = S3Config {
            bucket: bucket.into(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Create new S3 backend with custom configuration
    pub async fn with_config(config: S3Config) -> Result<Self> {
        if config.bucket.is_empty() {
            return Err(anyhow!("Bucket name cannot be empty"));
        }

        let mut aws_config_loader = aws_config::defaults(BehaviorVersion::latest());

        if let Some(region) = &config.region {
            aws_config_loader = aws_config_loader.region(Region::new(region.clone()));
        }

        let aws_config = aws_config_loader.load().await;

        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&aws_config);

        if let Some(endpoint) = &config.endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
        }

        if config.force_path_style {
            s3_config_builder = s3_config_builder.force_path_style(true);
        }

        let client = Client::from_conf(s3_config_builder.build());

        Ok(Self { client, config })
    }

    fn md5_base64(data: &[u8]) -> String {
        let sum = md5::compute(data);
        B64.encode(sum.0)
    }

    /// Put small objects directly (simpler than multipart upload)
    async fn put_object_simple(&self, key: &str, data: &[u8]) -> Result<()> {
        let mut attempt = 0;
        loop {
            attempt += 1;

            let mut request = self
                .client
                .put_object()
                .bucket(&self.config.bucket)
                .key(key)
                .body(SdkBody::from(data.to_vec()).into());

            if self.config.enable_md5 {
                let checksum = Self::md5_base64(data);
                request = request.content_md5(checksum);
            }

            match request.send().await {
                Ok(_) => return Ok(()),
                Err(_e) if attempt < self.config.max_retries => {
                    let delay = self.config.retry_base_delay * (1 << (attempt - 1));
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Handle multipart upload for large objects
    async fn multipart_upload(&self, key: &str, data: &[u8]) -> Result<()> {
        // Create multipart upload
        let create = self
            .client
            .create_multipart_upload()
            .bucket(&self.config.bucket)
            .key(key)
            .send()
            .await?;

        let upload_id = create
            .upload_id()
            .ok_or_else(|| anyhow!("Missing upload_id in create_multipart_upload response"))?
            .to_string();

        // Ensure we clean up the multipart upload if it fails
        let cleanup_on_drop = MultipartCleanupGuard {
            client: self.client.clone(),
            bucket: self.config.bucket.clone(),
            key: key.to_string(),
            upload_id: upload_id.clone(),
        };

        let data_arc = Arc::new(data.to_vec());
        let sem = Arc::new(tokio::sync::Semaphore::new(self.config.max_concurrency));

        // Concurrent upload of parts
        let mut parts = Vec::new();
        let total = data.len();
        let mut idx = 0usize;
        let mut part_number = 1i32;

        while idx < total {
            let end = (idx + self.config.part_size).min(total);
            let chunk_vec = data_arc.as_slice()[idx..end].to_vec();
            let client = self.client.clone();
            let bucket = self.config.bucket.clone();
            let key = key.to_string();
            let upload_id_cloned = upload_id.clone();
            let pn = part_number;
            let sem_cloned = sem.clone();
            let enable_md5 = self.config.enable_md5;
            let max_retries = self.config.max_retries;
            let retry_base_delay = self.config.retry_base_delay;

            let fut = async move {
                // Concurrency control
                let _permit = sem_cloned.acquire_owned().await.unwrap();
                let mut attempt = 0;

                loop {
                    attempt += 1;
                    let mut request = client
                        .upload_part()
                        .bucket(&bucket)
                        .key(&key)
                        .upload_id(&upload_id_cloned)
                        .part_number(pn)
                        .body(SdkBody::from(chunk_vec.clone()).into());

                    if enable_md5 {
                        let part_md5 = Self::md5_base64(&chunk_vec);
                        request = request.content_md5(part_md5);
                    }

                    match request.send().await {
                        Ok(ok) => break Ok((pn, ok.e_tag().map(|s| s.to_string()))),
                        Err(_e) if attempt < max_retries => {
                            let delay = retry_base_delay * (1 << (attempt - 1));
                            sleep(Duration::from_millis(delay)).await;
                            continue;
                        }
                        Err(e) => break Err(e),
                    }
                }
            };
            parts.push(fut);

            idx = end;
            part_number += 1;
        }

        // Execute all parts concurrently
        let results: Vec<(i32, Option<String>)> = match futures::future::try_join_all(parts).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };

        // Build completed parts
        let completed_parts = results
            .into_iter()
            .map(|(pn, etag)| {
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(pn)
                    .set_e_tag(etag)
                    .build()
            })
            .collect::<Vec<_>>();

        let completed = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        // Complete multipart upload
        self.client
            .complete_multipart_upload()
            .bucket(&self.config.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed)
            .send()
            .await?;

        // Disarm cleanup guard since upload succeeded
        std::mem::forget(cleanup_on_drop);

        Ok(())
    }
}

/// Guard to automatically clean up multipart uploads if they fail
struct MultipartCleanupGuard {
    client: Client,
    bucket: String,
    key: String,
    upload_id: String,
}

impl Drop for MultipartCleanupGuard {
    fn drop(&mut self) {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = self.key.clone();
        let upload_id = self.upload_id.clone();

        tokio::spawn(async move {
            let _ = client
                .abort_multipart_upload()
                .bucket(&bucket)
                .key(&key)
                .upload_id(&upload_id)
                .send()
                .await;
        });
    }
}

#[async_trait]
impl ObjectBackend for S3Backend {
    async fn put_object(&self, key: &str, data: &[u8]) -> Result<()> {
        // Small objects use direct put_object; large objects use multipart upload
        if data.len() <= self.config.part_size {
            return self.put_object_simple(key, data).await;
        }

        // Multipart upload for large objects
        self.multipart_upload(key, data).await
    }

    async fn get_object(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.config.bucket)
            .key(key)
            .send()
            .await;
        match resp {
            Ok(o) => {
                use tokio::io::AsyncReadExt;
                let mut body = o.body.into_async_read();
                let mut buf = Vec::new();
                body.read_to_end(&mut buf).await?;
                Ok(Some(buf))
            }
            Err(e) => {
                // Simplified: NoSuchKey returns None, other errors return Err
                let msg = format!("{e}");
                if msg.contains("NoSuchKey") || msg.contains("NotFound") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    async fn get_etag(&self, key: &str) -> Result<String> {
        let resp = self
            .client
            .head_object()
            .bucket(&self.config.bucket)
            .key(key)
            .send()
            .await?;
        Ok(resp.e_tag().unwrap_or_default().to_string())
    }

    async fn delete_object(&self, key: &str) -> Result<()> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self
                .client
                .delete_object()
                .bucket(&self.config.bucket)
                .key(key)
                .send()
                .await
            {
                Ok(_) => return Ok(()),
                Err(_e) if attempt < self.config.max_retries => {
                    let delay = self.config.retry_base_delay * (1 << (attempt - 1));
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}
