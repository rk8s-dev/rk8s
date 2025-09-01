use crate::api::xlinestore::XlineStore;
use crate::protocol:: RksMessage;
use anyhow::Result;
use quinn::Connection;
use std::sync::Arc;

pub async fn watch_delete(
    pod_name: String,
    conn: &Connection,
    _xline_store: &Arc<XlineStore>,
    _node_id: &str,
) -> Result<()> {
    let msg = RksMessage::DeletePod(pod_name.clone());
    let data = bincode::serialize(&msg)?;
    if let Ok(mut stream) = conn.open_uni().await {
        stream.write_all(&data).await?;
        stream.finish()?;
        println!("[watch_pods] send DeletePod for pod: {}", pod_name);
    }
    Ok(())
}

pub async fn user_delete(
    pod_name: String,
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
) -> Result<()> {
    xline_store.delete_pod(&
        
        
        pod_name).await?;
    println!("[user_delete] deleted pod {} (written to xline)",pod_name);

    let response = RksMessage::Ack;
    let data = bincode::serialize(&response)?;
    if let Ok(mut stream) = conn.open_uni().await {
        stream.write_all(&data).await?;
        stream.finish()?;
    }
    Ok(())
}
