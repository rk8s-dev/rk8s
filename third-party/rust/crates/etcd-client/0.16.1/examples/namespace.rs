//! Namespace example

use etcd_client::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = Client::connect(["localhost:2379"], None).await?;
    let mut kv_client = client.kv_client();
    let mut kv_client_prefix = KvClientPrefix::new(kv_client.clone(), "person/".into());

    kv_client_prefix.put("Alice", "15", None).await?;
    println!("put kv: {{Alice: 15}}");

    // get prefixed kv
    let resp = kv_client.get("person/Alice", None).await?;
    if let Some(kv) = resp.kvs().first() {
        println!(
            "Get prefixed kv: {{{}: {}}}",
            kv.key_str()?,
            kv.value_str()?
        );
    }

    // get kv
    let resp = kv_client_prefix.get("Alice", None).await?;
    if let Some(kv) = resp.kvs().first() {
        println!("Get kv: {{{}: {}}}", kv.key_str()?, kv.value_str()?);
    }

    // delete kv
    let resp = kv_client_prefix
        .delete("Alice", Some(DeleteOptions::new().with_prev_key()))
        .await?;
    if let Some(kv) = resp.prev_kvs().first() {
        println!("Delete kv: {{{}: {}}}", kv.key_str()?, kv.value_str()?);
    }

    Ok(())
}
