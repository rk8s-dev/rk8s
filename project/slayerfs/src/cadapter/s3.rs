//! S3 适配器：基于 aws-sdk-s3 的简化实现，支持大对象的分段上传、基础重试与校验。

use crate::cadapter::client::ObjectBackend;
use async_trait::async_trait;
use aws_sdk_s3::Client;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use md5;
use std::sync::Arc;
use tokio::{
    sync::Semaphore,
    time::{Duration, sleep},
};

/// S3 后端配置选项
#[derive(Debug, Clone)]
pub struct S3Config {
    /// 分段大小（字节），建议 8-64MiB
    pub part_size: usize,
    /// 最大并发分段上传数
    pub max_concurrency: usize,
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始重试延迟（毫秒）
    pub initial_retry_delay_ms: u64,
    /// 连接超时时间
    pub timeout: Duration,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            part_size: 8 * 1024 * 1024, // 8MB
            max_concurrency: 8,
            max_retries: 3,
            initial_retry_delay_ms: 100,
            timeout: Duration::from_secs(30),
        }
    }
}

#[allow(dead_code)]
pub struct S3Backend {
    client: Client,
    bucket: String,
    config: S3Config,
}

#[allow(dead_code)]
impl S3Backend {
    pub async fn new(
        bucket: impl Into<String>,
        config: S3Config,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conf = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = Client::new(&conf);
        Ok(Self {
            client,
            bucket: bucket.into(),
            config,
        })
    }

    fn md5_base64(data: &[u8]) -> String {
        let sum = md5::compute(data);
        B64.encode(sum.0)
    }
}

impl S3Backend {
    async fn execute_with_retry<T, F, Fut, E>(
        &self,
        operation: F,
        operation_name: &'static str,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display + std::fmt::Debug + Send + Sync + 'static,
    {
        let mut attempt = 0;
        let max_retries = self.config.max_retries;
        loop {
            attempt += 1;
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if attempt > max_retries {
                        return Err(Box::new(std::io::Error::other(format!(
                            "{operation_name} failed after {max_retries} attempts: {e}"
                        ))));
                    }

                    let delay_ms = self.config.initial_retry_delay_ms * 2u64.pow(attempt - 1);
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload_part(
        &self,
        client: Client,
        bucket: String,
        key: String,
        upload_id: String,
        part_number: i32,
        data: Vec<u8>,
        semaphore: Arc<Semaphore>,
    ) -> Result<(i32, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
        let _permit = semaphore.acquire().await.unwrap();
        let checksum = Self::md5_base64(&data);

        let operation = || async {
            client
                .upload_part()
                .bucket(&bucket)
                .key(&key)
                .upload_id(&upload_id)
                .part_number(part_number)
                .content_md5(checksum.clone())
                .body(data.clone().into())
                .send()
                .await
        };

        self.execute_with_retry(operation, "upload_part")
            .await
            .map(|resp| (part_number, resp.e_tag().map(|s| s.to_string())))
    }
}

#[async_trait]
impl ObjectBackend for S3Backend {
    async fn put_object(
        &self,
        key: &str,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 小对象直接 put_object；大对象走 multipart。
        if data.len() <= self.config.part_size {
            // 简单重试 3 次。
            let checksum = Self::md5_base64(data);
            let mut attempt = 0;
            loop {
                attempt += 1;
                let req = self
                    .client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(key)
                    .body(data.to_owned().into())
                    .content_md5(checksum.clone());
                match req.send().await {
                    Ok(_) => return Ok(()),
                    Err(_e) if attempt < 3 => {
                        sleep(Duration::from_millis(100 * attempt)).await;
                        continue;
                    }
                    Err(e) => return Err(Box::new(e)),
                }
            }
        }

        // multipart upload
        let create = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;
        let upload_id = create.upload_id().unwrap_or_default().to_string();
        let data_arc = Arc::new(data.to_vec());
        let sem = Arc::new(tokio::sync::Semaphore::new(self.config.max_concurrency));

        // 并发上传各分片
        let mut parts = Vec::new();
        let total = data.len();
        let mut idx = 0usize;
        let mut part_number = 1i32;

        while idx < total {
            let end = (idx + self.config.part_size).min(total);
            let chunk_vec = data_arc.as_slice()[idx..end].to_vec();
            let client = self.client.clone();
            let bucket = self.bucket.clone();
            let key = key.to_string();
            let upload_id_cloned = upload_id.clone();
            let pn = part_number;

            let fut = self.upload_part(
                client,
                bucket,
                key,
                upload_id_cloned,
                pn,
                chunk_vec,
                sem.clone(),
            );

            parts.push(fut);

            idx = end;
            part_number += 1;
        }

        // 并发执行（简化：不做限流，后续可换 FuturesUnordered + semaphore 实现）
        let results: Vec<(i32, Option<String>)> = match futures::future::try_join_all(parts).await {
            Ok(v) => v,
            Err(e) => {
                self.client
                    .abort_multipart_upload()
                    .bucket(self.bucket.clone())
                    .key(key)
                    .upload_id(upload_id)
                    .send()
                    .await
                    .unwrap();
                return Err(e);
            }
        };

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

        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed)
            .send()
            .await?;

        Ok(())
    }

    async fn get_object(
        &self,
        key: &str,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
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
                // 简化：NoSuchKey 返回 None，其他错误返回 Err
                let msg = format!("{e}");
                if msg.contains("NoSuchKey") {
                    Ok(None)
                } else {
                    Err(Box::new(e))
                }
            }
        }
    }
}
