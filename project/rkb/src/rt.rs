use anyhow::Context;

pub fn block_on<F, Fut>(f: F) -> anyhow::Result<Fut>
where
    F: Future<Output = Fut>,
{
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .with_context(|| "Failed to build Tokio runtime")?;
    Ok(rt.block_on(f))
}
