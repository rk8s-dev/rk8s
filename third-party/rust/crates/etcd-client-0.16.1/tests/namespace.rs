mod testing;

use crate::testing::{get_client, Result};
use etcd_client::{DeleteOptions, KvClientPrefix};

#[tokio::test]
async fn test_namespace_kv() -> Result<()> {
    let mut client = get_client().await?;
    let mut c = KvClientPrefix::new(client.kv_client(), "foo/".into());

    c.put("abc", "bar", None).await?;

    // get kv
    {
        let kvs = c.get("abc", None).await?.take_kvs();
        assert_eq!(kvs.len(), 1);
        assert_eq!(kvs[0].key(), b"abc");
        assert_eq!(kvs[0].value(), b"bar");
    }

    // get prefixed kv
    {
        let kvs = client.get("foo/abc", None).await?.take_kvs();
        assert_eq!(kvs.len(), 1);
        assert_eq!(kvs[0].key(), b"foo/abc");
        assert_eq!(kvs[0].value(), b"bar");
    }

    // delete kv
    {
        let resp = c
            .delete("abc", Some(DeleteOptions::new().with_prev_key()))
            .await?;
        assert_eq!(resp.deleted(), 1);
        let kvs = resp.prev_kvs();
        assert_eq!(kvs[0].key(), b"abc");
        assert_eq!(kvs[0].value(), b"bar");
    }

    Ok(())
}
