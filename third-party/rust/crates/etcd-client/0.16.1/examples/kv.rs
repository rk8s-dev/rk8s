//! KV example.

use etcd_client::*;

#[derive(Debug)]
struct KV {
    key: String,
    value: String,
}

impl KV {
    #[inline]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        KV {
            key: key.into(),
            value: value.into(),
        }
    }

    #[inline]
    pub fn key(&self) -> &str {
        &self.key
    }

    #[inline]
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;

    let alice = KV::new("Alice", "15");
    let bob = KV::new("Bob", "20");
    let chris = KV::new("Chris", "16");

    // put kv
    let resp = client.put(alice.key(), alice.value(), None).await?;
    println!("put kv: {alice:?}");
    let revision = resp.header().unwrap().revision();
    client.put(bob.key(), bob.value(), None).await?;
    println!("put kv: {bob:?}");
    client.put(chris.key(), chris.value(), None).await?;
    println!("put kv: {chris:?}");
    println!();

    // get kv
    let resp = client.get(bob.key(), None).await?;
    if let Some(kv) = resp.kvs().first() {
        println!("Get kv: {{{}: {}}}", kv.key_str()?, kv.value_str()?);
        println!();
    }

    // get all kv pairs
    println!("Get all users:");
    let resp = client
        .get("", Some(GetOptions::new().with_all_keys()))
        .await?;
    for kv in resp.kvs() {
        println!("\t{{{}: {}}}", kv.key_str()?, kv.value_str()?);
    }
    println!();

    // delete kv
    let resp = client
        .delete(chris.key(), Some(DeleteOptions::new().with_prev_key()))
        .await?;
    if let Some(kv) = resp.prev_kvs().first() {
        println!("Delete kv: {{{}: {}}}", kv.key_str()?, kv.value_str()?);
        println!();
    }

    // transaction
    let resp = client.get(alice.key(), None).await?;
    if let Some(kv) = resp.kvs().first() {
        println!(
            "Before transaction: {{{}: {}}}",
            kv.key_str()?,
            kv.value_str()?
        );
    }
    let txn = Txn::new()
        .when(vec![Compare::value(
            alice.key(),
            CompareOp::Equal,
            alice.value(),
        )])
        .and_then(vec![TxnOp::put(
            alice.key(),
            "18",
            Some(PutOptions::new().with_ignore_lease()),
        )]);
    println!("transaction: {txn:?}");
    let resp = client.txn(txn).await?;
    for op_resp in resp.op_responses() {
        println!("transaction resp: {op_resp:?}");
    }
    let resp = client.get(alice.key(), None).await?;
    if let Some(kv) = resp.kvs().first() {
        println!(
            "After transaction: {{{}: {}}}",
            kv.key_str()?,
            kv.value_str()?
        );
    }
    println!();

    // compact
    client
        .compact(revision, Some(CompactionOptions::new().with_physical()))
        .await?;
    println!("Compact to revision {revision}.");

    Ok(())
}
