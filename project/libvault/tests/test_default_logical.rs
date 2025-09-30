use std::{collections::HashMap, env, fs};

use go_defer::defer;
use rusty_vault::{
    core::{Core, SealConfig},
    logical::{Operation, Request},
    storage, RustyVault,
};
use serde_json::{json, Map, Value};

#[maybe_async::maybe_async]
async fn test_read_api(core: &Core, token: &str, path: &str, is_ok: bool, expect: Option<Map<String, Value>>) {
    let mut req = Request::new(path);
    req.operation = Operation::Read;
    req.client_token = token.to_string();
    let resp = core.handle_request(&mut req).await;
    assert_eq!(resp.is_ok(), is_ok);
    if expect.is_some() {
        let resp = resp.unwrap();
        assert!(resp.is_some());
        assert_eq!(resp.unwrap().data.as_ref().unwrap(), expect.as_ref().unwrap());
    } else if is_ok {
        let resp = resp.unwrap();
        assert!(resp.is_none());
    }
}

#[maybe_async::maybe_async]
async fn test_write_api(core: &Core, token: &str, path: &str, is_ok: bool, data: Option<Map<String, Value>>) {
    let mut req = Request::new(path);
    req.operation = Operation::Write;
    req.client_token = token.to_string();
    req.body = data;

    let ret = core.handle_request(&mut req).await;

    assert_eq!(ret.is_ok(), is_ok);
}

#[maybe_async::maybe_async]
async fn test_delete_api(core: &Core, token: &str, path: &str, is_ok: bool) {
    let mut req = Request::new(path);
    req.operation = Operation::Delete;
    req.client_token = token.to_string();

    let ret = core.handle_request(&mut req).await;

    assert_eq!(ret.is_ok(), is_ok);
}

#[maybe_async::maybe_async]
async fn test_list_api(core: &Core, token: &str, path: &str, is_ok: bool, keys_len: usize) {
    let mut req = Request::new(path);
    req.operation = Operation::List;
    req.client_token = token.to_string();
    let resp = core.handle_request(&mut req).await;
    assert_eq!(resp.is_ok(), is_ok);
    if is_ok {
        let resp = resp.unwrap();
        assert!(resp.is_some());
        let data = resp.unwrap().data.unwrap();
        let keys = data["keys"].as_array();
        assert_eq!(keys.unwrap().len(), keys_len);
    }
}

#[maybe_async::maybe_async]
async fn test_default_secret(core: &Core, token: &str) {
    // create secret
    let kv_data = json!({
        "foo": "bar",
        "zip": "zap",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "secret/goo", true, Some(kv_data.clone())).await;

    // get secret
    test_read_api(core, token, "secret/goo", true, Some(kv_data)).await;
    test_read_api(core, token, "secret/foo", true, None).await;
    test_read_api(core, token, "secret1/foo", false, None).await;

    // list secret
    test_list_api(core, token, "secret/", true, 1).await;
}

#[maybe_async::maybe_async]
async fn test_kv_logical_backend(core: &Core, token: &str) {
    // mount kv backend to path: kv/
    let mount_data = json!({
        "type": "kv",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/mounts/kv/", true, Some(mount_data)).await;

    let kv_data = json!({
        "foo": "bar",
        "zip": "zap",
    })
    .as_object()
    .unwrap()
    .clone();

    test_read_api(core, token, "secret/foo", true, None).await;

    // create secret
    test_write_api(core, token, "kv/secret", true, Some(kv_data.clone())).await;
    test_write_api(core, token, "kv1/secret", false, Some(kv_data.clone())).await;

    // get secret
    test_read_api(core, token, "kv/secret", true, Some(kv_data)).await;
    test_read_api(core, token, "kv/secret1", true, None).await;

    // list secret
    test_list_api(core, token, "kv/", true, 1).await;

    // update secret
    let kv_data = json!({
        "foo": "bar",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "kv/secret", true, Some(kv_data.clone())).await;

    // check whether the secret is updated successfully
    test_read_api(core, token, "kv/secret", true, Some(kv_data)).await;

    // add secret
    let kv_data = json!({
        "foo": "bar",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "kv/foo", true, Some(kv_data.clone())).await;

    // list secret
    test_list_api(core, token, "kv/", true, 2).await;

    // delete secret
    test_delete_api(core, token, "kv/secret", true).await;
    test_delete_api(core, token, "kv/secret11", true).await;

    // list secret again
    test_list_api(core, token, "kv/", true, 1).await;

    // remount kv backend to path: kv/
    let remount_data = json!({
        "from": "kv",
        "to": "vk",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/remount", true, Some(remount_data)).await;

    // get secret from new mount path
    test_read_api(core, token, "vk/foo", true, Some(kv_data)).await;

    // unmount
    test_delete_api(core, token, "sys/mounts/vk/", true).await;

    // Getting the secret should fail
    test_read_api(core, token, "vk/foo", false, None).await;
}

#[maybe_async::maybe_async]
async fn test_sys_mount_feature(core: &Core, token: &str) {
    // test api: "mounts"
    let mut req = Request::new("sys/mounts");
    req.operation = Operation::Read;
    req.client_token = token.to_string();
    let resp = core.handle_request(&mut req).await;
    assert!(resp.is_ok());
    let resp = resp.unwrap();
    assert!(resp.is_some());
    let data = resp.unwrap().data;
    assert!(data.is_some());
    assert_eq!(data.as_ref().unwrap().len(), 3);

    // test api: "mounts/kv" with valid type
    let mount_data = json!({
        "type": "kv",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/mounts/kv/", true, Some(mount_data.clone())).await;

    // test api: "mounts/kv" with path conflict
    test_write_api(core, token, "sys/mounts/kv/", false, Some(mount_data)).await;

    // test api: "mounts/nope" with valid type
    let mount_data = json!({
        "type": "nope",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/mounts/nope/", false, Some(mount_data)).await;

    // test api: "remount" with valid path
    let remount_data = json!({
        "from": "kv",
        "to": "vk",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/remount", true, Some(remount_data)).await;

    // test api: "remount" with invalid path
    let remount_data = json!({
        "from": "unknow",
        "to": "vvk",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/remount", false, Some(remount_data)).await;

    // test api: "remount" with dis-path conflict
    let remount_data = json!({
        "from": "vk",
        "to": "secret",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/remount", false, Some(remount_data)).await;

    // test api: "remount" with protect path
    let remount_data = json!({
        "from": "sys",
        "to": "foo",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/remount", false, Some(remount_data)).await;

    // test api: "remount" with default src-path
    let remount_data = json!({
        "from": "secret",
        "to": "bar",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/remount", true, Some(remount_data)).await;
}

#[maybe_async::maybe_async]
async fn test_sys_raw_api_feature(core: &Core, token: &str) {
    // test raw read
    let mut req = Request::new("sys/raw/core/mounts");
    req.operation = Operation::Read;
    req.client_token = token.to_string();
    let resp = core.handle_request(&mut req).await;
    assert!(resp.is_ok());
    let resp = resp.unwrap();
    let data = resp.unwrap().data;
    assert!(data.is_some());
    assert_ne!(data.as_ref().unwrap().len(), 0);
    assert!(data.as_ref().unwrap()["value"].as_str().unwrap().starts_with('{'));

    // test raw write
    let test_data = json!({
        "value": "my test data",
    })
    .as_object()
    .unwrap()
    .clone();
    test_write_api(core, token, "sys/raw/test", true, Some(test_data.clone())).await;

    // test raw read again
    let mut req = Request::new("sys/raw/test");
    req.operation = Operation::Read;
    req.client_token = token.to_string();
    let resp = core.handle_request(&mut req).await;
    assert!(resp.is_ok());
    let resp = resp.unwrap();
    let data = resp.unwrap().data;
    assert!(data.is_some());
    assert_eq!(data.as_ref().unwrap()["value"].as_str().unwrap(), test_data["value"].as_str().unwrap());

    // test raw delete
    test_delete_api(core, token, "sys/raw/test", true).await;

    // test raw read again
    test_read_api(core, token, "sys/raw/test", true, None).await;
}

#[maybe_async::maybe_async]
async fn test_rvualt_mount(rvault: &RustyVault, token: &str) {
    let ret = rvault.mount(Some(token), "kv9/test", "kv").await;
    assert!(ret.is_ok());

    let ret = rvault
        .write(
            Some(token),
            "kv9/test/foo",
            Some(
                json!({
                    "foo": "bar",
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await;
    assert!(ret.is_ok());

    let ret = rvault.read(Some(token), "kv9/test/foo").await;
    assert!(ret.is_ok());
    let ret = ret.unwrap();
    assert!(ret.is_some());
    let data = ret.unwrap().data;
    assert!(data.is_some());
    assert_eq!(data.as_ref().unwrap()["foo"].as_str().unwrap(), "bar");

    let ret = rvault
        .write(
            Some(token),
            "kv9/test/bar/foo",
            Some(
                json!({
                    "bar": "foo",
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await;
    assert!(ret.is_ok());

    let ret = rvault.read(Some(token), "kv9/test/bar/foo").await;
    assert!(ret.is_ok());
    let ret = ret.unwrap();
    assert!(ret.is_some());
    let data = ret.unwrap().data;
    assert!(data.is_some());
    assert_eq!(data.as_ref().unwrap()["bar"].as_str().unwrap(), "foo");

    let ret = rvault.list(Some(token), "kv9/test/").await;
    assert!(ret.is_ok());
    let ret = ret.unwrap();
    assert!(ret.is_some());
    let data = ret.unwrap().data;
    assert!(data.is_some());
    assert_eq!(data.as_ref().unwrap()["keys"].as_array().unwrap().len(), 2);

    let ret = rvault.delete(Some(token), "kv9/test/foo", None).await;
    assert!(ret.is_ok());

    let ret = rvault.list(Some(token), "kv9/test/").await;
    assert!(ret.is_ok());
    let ret = ret.unwrap();
    assert!(ret.is_some());
    let data = ret.unwrap().data;
    assert!(data.is_some());
    assert_eq!(data.as_ref().unwrap()["keys"].as_array().unwrap().len(), 1);

    let ret = rvault.unmount(Some(token), "kv9/test").await;
    assert!(ret.is_ok());

    let ret = rvault.list(Some(token), "kv9/test/").await;
    assert!(ret.is_err());
}

#[maybe_async::maybe_async]
async fn test_sys_logical_backend(core: &Core, token: &str) {
    test_sys_mount_feature(core, token).await;
    test_sys_raw_api_feature(core, token).await;
}

#[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
async fn test_default_logical() {
    use rusty_vault::RustyVault;

    let dir = env::temp_dir().join("rusty_vault_core_init");
    let _ = fs::remove_dir_all(&dir);
    assert!(fs::create_dir(&dir).is_ok());
    defer! (
        assert!(fs::remove_dir_all(&dir).is_ok());
    );

    let mut root_token = String::new();
    println!("root_token: {:?}", root_token);

    let mut conf: HashMap<String, Value> = HashMap::new();
    conf.insert("path".to_string(), Value::String(dir.to_string_lossy().into_owned()));

    let backend = storage::new_backend("file", &conf).unwrap();

    let rvault = RustyVault::new(backend, None).unwrap();
    let core = rvault.core.load();

    let seal_config = SealConfig { secret_shares: 10, secret_threshold: 5 };

    let result = rvault.init(&seal_config).await;
    assert!(result.is_ok());
    let init_result = result.unwrap();
    println!("init_result: {:?}", init_result);

    let mut unsealed = false;
    for i in 0..seal_config.secret_threshold {
        let key = &init_result.secret_shares[i as usize];
        let unseal = rvault.unseal(&[key]).await;
        assert!(unseal.is_ok());
        unsealed = unseal.unwrap();
    }

    root_token = init_result.root_token.clone();

    assert!(unsealed);

    {
        println!("root_token: {:?}", root_token);
        test_default_secret(&core, &root_token).await;
        test_kv_logical_backend(&core, &root_token).await;
        test_sys_logical_backend(&core, &root_token).await;
        test_rvualt_mount(&rvault, &root_token).await;
    }
}
