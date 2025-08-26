//! S3 适配器：基于 aws-sdk-s3 的简化实现，支持大对象的分段上传、基础重试与校验。

use crate::cadapter::client::ObjectBackend;
use async_trait::async_trait;
use aws_sdk_s3::Client;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use md5;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[allow(dead_code)]
pub struct S3Backend {
    client: Client,
    bucket: String,
    /// 分段大小（建议 8-64MiB）。
    part_size: usize,
    /// 最大并发分段上传数。
    max_concurrency: usize,
}

#[allow(dead_code)]
impl S3Backend {
    pub async fn new(bucket: impl Into<String>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conf = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = Client::new(&conf);
        Ok(Self { client, bucket: bucket.into(), part_size: 8 * 1024 * 1024, max_concurrency: 4 })
    }

    fn md5_base64(data: &[u8]) -> String {
        let sum = md5::compute(data);
        B64.encode(sum.0)
    }
}

#[async_trait]
impl ObjectBackend for S3Backend {
    async fn put_object(&self, key: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 小对象直接 put_object；大对象走 multipart。
        if data.len() <= self.part_size {
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
    let sem = Arc::new(tokio::sync::Semaphore::new(self.max_concurrency));

        // 并发上传各分片
        let mut parts = Vec::new();
        let total = data.len();
        let mut idx = 0usize;
        let mut part_number = 1i32;

        while idx < total {
            let end = (idx + self.part_size).min(total);
            let chunk_vec = data_arc.as_slice()[idx..end].to_vec();
            let client = self.client.clone();
            let bucket = self.bucket.clone();
            let key = key.to_string();
            let upload_id_cloned = upload_id.clone();
            let pn = part_number;
            let sem_cloned = sem.clone();

            let fut = async move {
                // 并发控制
                let _permit = sem_cloned.acquire_owned().await.unwrap();
                let mut attempt = 0;
                let part_md5 = super::s3::S3Backend::md5_base64(&chunk_vec);
                loop {
                    attempt += 1;
                    let resp = client
                        .upload_part()
                        .bucket(&bucket)
                        .key(&key)
                        .upload_id(&upload_id_cloned)
                        .part_number(pn)
                        .content_md5(part_md5.clone())
                        .body(chunk_vec.clone().into())
                        .send()
                        .await;
                    match resp {
                        Ok(ok) => break Ok((pn, ok.e_tag().map(|s| s.to_string()))),
                        Err(_e) if attempt < 3 => {
                            sleep(Duration::from_millis(100 * attempt)).await;
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

        // 并发执行（简化：不做限流，后续可换 FuturesUnordered + semaphore 实现）
        let results: Vec<(i32, Option<String>)> = match futures::future::try_join_all(parts).await {
            Ok(v) => v,
            Err(e) => return Err(Box::new(e)),
        };

        let completed_parts = results
            .into_iter()
            .map(|(pn, etag)| aws_sdk_s3::types::CompletedPart::builder().part_number(pn).set_e_tag(etag).build())
            .collect::<Vec<_>>();

        let completed = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        self
            .client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed)
            .send()
            .await?;

        Ok(())
    }

    async fn get_object(&self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
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
