//! Watch example

use etcd_client::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;

    client.put("foo", "bar", None).await?;
    println!("put kv: {{foo: bar}}");

    client.put("foo1", "bar1", None).await?;
    println!("put kv: {{foo1: bar1}}");

    let (mut watcher, mut stream) = client.watch("foo", None).await?;
    println!("create watcher {}", watcher.watch_id());
    println!();

    client.put("foo", "bar2", None).await?;
    watcher.request_progress().await?;
    client.delete("foo", None).await?;

    watcher.watch("foo1", None).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    client.put("foo1", "bar2", None).await?;
    client.delete("foo1", None).await?;

    let mut watch_count = 2;

    while let Some(resp) = stream.message().await? {
        println!("[{}] receive watch response", resp.watch_id());
        println!("compact revision: {}", resp.compact_revision());

        if resp.created() {
            println!("watcher created: {}", resp.watch_id());
        }

        if resp.canceled() {
            watch_count -= 1;
            println!("watch canceled: {}", resp.watch_id());
            if watch_count == 0 {
                break;
            }
        }

        for event in resp.events() {
            println!("event type: {:?}", event.event_type());
            if let Some(kv) = event.kv() {
                println!("kv: {{{}: {}}}", kv.key_str()?, kv.value_str()?);
            }

            if EventType::Delete == event.event_type() {
                watcher.cancel_by_id(resp.watch_id()).await?;
            }
        }

        println!();
    }

    Ok(())
}
