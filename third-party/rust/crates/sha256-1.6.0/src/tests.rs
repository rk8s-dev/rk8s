use super::*;

#[cfg(feature = "native_openssl")]
#[test]
fn test_openssl() {
    let input = Path::new("./foo.file");
    let f = fs::File::open(input).unwrap();
    let reader = BufReader::new(f);
    let sha = openssl::sha::Sha256::new();
    assert_eq!(
        "433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1",
        calc(reader, sha).unwrap()
    );
}

#[test]
fn test_sha256() {
    let input = Path::new("./foo.file");
    let f = fs::File::open(input).unwrap();
    let reader = BufReader::new(f);
    let sha = Sha256::new();
    assert_eq!(
        "433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1",
        calc(reader, sha).unwrap()
    );
}

#[cfg(all(feature = "async", feature = "native_openssl"))]
#[tokio::test]
async fn test_async_openssl() {
    let input = Path::new("./foo.file");
    let f = tokio::fs::File::open(input).await.unwrap();
    let reader = tokio::io::BufReader::new(f);
    let sha = openssl::sha::Sha256::new();
    assert_eq!(
        "433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1",
        async_calc(reader, sha).await.unwrap()
    );
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_async() {
    let input = Path::new("./foo.file");
    let f = tokio::fs::File::open(input).await.unwrap();
    let reader = tokio::io::BufReader::new(f);
    let sha = Sha256::new();
    assert_eq!(
        "433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1",
        async_calc(reader, sha).await.unwrap()
    );
}

#[cfg(all(feature = "async", feature = "native_openssl"))]
#[tokio::test]
async fn test_try_async_openssl_digest() {
    let hash = try_async_openssl_digest("./foo.file").await.unwrap();
    assert_eq!(
        "433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1",
        hash
    );
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_try_async_digest() {
    let hash = try_async_digest("./foo.file").await.unwrap();
    assert_eq!(
        "433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1",
        hash
    );
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_async_parity() {
    let bytes = (0..0x1000).map(|v| (v % 256) as u8).collect::<Vec<_>>();
    let val = digest(&bytes);

    let async_res = {
        let bytes = &bytes;
        // We want to force Poll::Pending on reads during async_calc, which may break parity
        // between sync and async hashing.
        let (client, mut server) = tokio::io::duplex(64);
        let reader = tokio::io::BufReader::new(client);
        let sha = Sha256::new();

        use tokio::io::AsyncWriteExt;
        tokio::join! {
            async_calc(reader, sha),
            async move {
                server.write_all(&bytes[..]).await.unwrap();
                core::mem::drop(server);
            }
        }
        .0
        .unwrap()
    };

    let sync_res = {
        let reader = BufReader::new(&*bytes);
        let sha = Sha256::new();
        calc(reader, sha).unwrap()
    };
    assert_eq!(val, async_res);
    assert_eq!(async_res, sync_res);
}

#[cfg(all(feature = "async", feature = "native_openssl"))]
#[tokio::test]
async fn test_async_parity_openssl() {
    let bytes = (0..0x1000).map(|v| (v % 256) as u8).collect::<Vec<_>>();
    let val = digest(&bytes);

    let async_res = {
        let bytes = &bytes;
        // We want to force Poll::Pending on reads during async_calc, which may break parity
        // between sync and async hashing.
        let (client, mut server) = tokio::io::duplex(64);
        let reader = tokio::io::BufReader::new(client);
        let sha = OpenSslSha256::new();

        use tokio::io::AsyncWriteExt;
        tokio::join! {
            async_calc(reader, sha),
            async move {
                server.write_all(&bytes[..]).await.unwrap();
                core::mem::drop(server);
            }
        }
        .0
        .unwrap()
    };

    let sync_res = {
        let reader = BufReader::new(&*bytes);
        let sha = OpenSslSha256::new();
        calc(reader, sha).unwrap()
    };
    assert_eq!(val, async_res);
    assert_eq!(async_res, sync_res);
}
