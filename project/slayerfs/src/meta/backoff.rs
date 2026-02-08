use crate::meta::store::MetaError;
use rand::{RngCore, rng};
use std::{future::Future, time::Duration};

pub(crate) async fn backoff<F, Fut, R>(max_retries: u64, mut f: F) -> Result<R, MetaError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<R, MetaError>>,
{
    for c in 0..max_retries {
        match f().await {
            Ok(r) => return Ok(r),
            Err(MetaError::ContinueRetry) => {}
            Err(e) => return Err(e),
        }

        let random_delta = rng().next_u64() % 20;
        tokio::time::sleep(Duration::from_millis(20 + (1 << c) + random_delta)).await;
    }

    Err(MetaError::MaxRetriesExceeded)
}

#[macro_export]
macro_rules! backoff {
    ($max_retries:expr, $retry_block:block) => {
        async {
            for c in 0..$max_retries {
                match $retry_block.await {
                    Ok(r) => return Ok(r),
                    Err(e) if matches!(e, $crate::meta::store::MetaError::ContinueRetry) => {}
                    Err(e) => return Err(e),
                }

                let random_delta = rand::rng().next_u64() % 20;
                tokio::time::sleep(std::time::Duration::from_millis(
                    20 + (1 << c) + random_delta,
                ))
                .await;
            }

            Err($crate::meta::store::MetaError::MaxRetriesExceeded)
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::meta::backoff::backoff;
    use crate::meta::store::MetaError;
    use rand::RngCore;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[tokio::test]
    async fn test_backoff() -> anyhow::Result<()> {
        let try_my_fortune = 8;
        let current = AtomicI32::new(0);

        let maybe_failed = || async {
            if current.load(Ordering::Relaxed) < try_my_fortune {
                current.fetch_add(1, Ordering::Relaxed);
                Err(MetaError::ContinueRetry)
            } else {
                Ok(0)
            }
        };

        backoff(10, maybe_failed).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_easy_backoff_macro() -> anyhow::Result<()> {
        let try_my_fortune = 8;
        let mut current = 0;

        backoff! {
            10,
            {
                async {
                    if current < try_my_fortune {
                        current += 1;
                        Err(MetaError::ContinueRetry)
                    } else {
                        Ok(0)
                    }
                }
            }
        }
        .await?;
        Ok(())
    }
}
