mod testing;

use crate::testing::{get_client, Result, DEFAULT_TEST_ENDPOINT};
use etcd_client::{
    AlarmAction, AlarmOptions, AlarmType, Client, Compare, CompareOp, ConnectOptions,
    DeleteOptions, EventType, GetOptions, LeaseGrantOptions, MemberAddOptions, Permission,
    PermissionType, ProclaimOptions, PutOptions, ResignOptions, RoleRevokePermissionOptions, Txn,
    TxnOp, TxnOpResponse, UserAddOptions,
};

#[tokio::test]
async fn test_put() -> Result<()> {
    let mut client = get_client().await?;
    client.put("put", "123", None).await?;

    // overwrite with prev key
    {
        let resp = client
            .put("put", "456", Some(PutOptions::new().with_prev_key()))
            .await?;
        let prev_key = resp.prev_key();
        assert!(prev_key.is_some());
        let prev_key = prev_key.unwrap();
        assert_eq!(prev_key.key(), b"put");
        assert_eq!(prev_key.value(), b"123");
    }

    // overwrite again with prev key
    {
        let resp = client
            .put("put", "789", Some(PutOptions::new().with_prev_key()))
            .await?;
        let prev_key = resp.prev_key();
        assert!(prev_key.is_some());
        let prev_key = prev_key.unwrap();
        assert_eq!(prev_key.key(), b"put");
        assert_eq!(prev_key.value(), b"456");
    }

    Ok(())
}

#[tokio::test]
async fn test_get() -> Result<()> {
    let mut client = get_client().await?;
    client.put("get10", "10", None).await?;
    client.put("get11", "11", None).await?;
    client.put("get20", "20", None).await?;
    client.put("get21", "21", None).await?;

    // get key
    {
        let resp = client.get("get11", None).await?;
        assert_eq!(resp.count(), 1);
        assert!(!resp.more());
        assert_eq!(resp.kvs().len(), 1);
        assert_eq!(resp.kvs()[0].key(), b"get11");
        assert_eq!(resp.kvs()[0].value(), b"11");
    }

    // get from key
    {
        let resp = client
            .get(
                "get11",
                Some(GetOptions::new().with_from_key().with_limit(2)),
            )
            .await?;
        assert!(resp.more());
        assert_eq!(resp.kvs().len(), 2);
        assert_eq!(resp.kvs()[0].key(), b"get11");
        assert_eq!(resp.kvs()[0].value(), b"11");
        assert_eq!(resp.kvs()[1].key(), b"get20");
        assert_eq!(resp.kvs()[1].value(), b"20");
    }

    // get prefix keys
    {
        let resp = client
            .get("get1", Some(GetOptions::new().with_prefix()))
            .await?;
        assert_eq!(resp.count(), 2);
        assert!(!resp.more());
        assert_eq!(resp.kvs().len(), 2);
        assert_eq!(resp.kvs()[0].key(), b"get10");
        assert_eq!(resp.kvs()[0].value(), b"10");
        assert_eq!(resp.kvs()[1].key(), b"get11");
        assert_eq!(resp.kvs()[1].value(), b"11");
    }

    Ok(())
}

#[tokio::test]
async fn test_delete() -> Result<()> {
    let mut client = get_client().await?;
    client.put("del10", "10", None).await?;
    client.put("del11", "11", None).await?;
    client.put("del20", "20", None).await?;
    client.put("del21", "21", None).await?;
    client.put("del31", "31", None).await?;
    client.put("del32", "32", None).await?;

    // delete key
    {
        let resp = client.delete("del11", None).await?;
        assert_eq!(resp.deleted(), 1);
        let resp = client
            .get("del11", Some(GetOptions::new().with_count_only()))
            .await?;
        assert_eq!(resp.count(), 0);
    }

    // delete a range of keys
    {
        let resp = client
            .delete("del11", Some(DeleteOptions::new().with_range("del22")))
            .await?;
        assert_eq!(resp.deleted(), 2);
        let resp = client
            .get(
                "del11",
                Some(GetOptions::new().with_range("del22").with_count_only()),
            )
            .await?;
        assert_eq!(resp.count(), 0);
    }

    // delete key with prefix
    {
        let resp = client
            .delete("del3", Some(DeleteOptions::new().with_prefix()))
            .await?;
        assert_eq!(resp.deleted(), 2);
        let resp = client
            .get("del32", Some(GetOptions::new().with_count_only()))
            .await?;
        assert_eq!(resp.count(), 0);
    }

    Ok(())
}

#[tokio::test]
async fn test_compact() -> Result<()> {
    let mut client = get_client().await?;
    let rev0 = client
        .put("compact", "0", None)
        .await?
        .header()
        .unwrap()
        .revision();
    let rev1 = client
        .put("compact", "1", None)
        .await?
        .header()
        .unwrap()
        .revision();

    // before compacting
    let rev0_resp = client
        .get("compact", Some(GetOptions::new().with_revision(rev0)))
        .await?;
    assert_eq!(rev0_resp.kvs()[0].value(), b"0");
    let rev1_resp = client
        .get("compact", Some(GetOptions::new().with_revision(rev1)))
        .await?;
    assert_eq!(rev1_resp.kvs()[0].value(), b"1");

    client.compact(rev1, None).await?;

    // after compacting
    let result = client
        .get("compact", Some(GetOptions::new().with_revision(rev0)))
        .await;
    assert!(result.is_err());
    let rev1_resp = client
        .get("compact", Some(GetOptions::new().with_revision(rev1)))
        .await?;
    assert_eq!(rev1_resp.kvs()[0].value(), b"1");

    Ok(())
}

#[tokio::test]
async fn test_txn() -> Result<()> {
    let mut client = get_client().await?;
    client.put("txn01", "01", None).await?;

    // transaction 1
    {
        let resp = client
            .txn(
                Txn::new()
                    .when(&[Compare::value("txn01", CompareOp::Equal, "01")][..])
                    .and_then(
                        &[TxnOp::put(
                            "txn01",
                            "02",
                            Some(PutOptions::new().with_prev_key()),
                        )][..],
                    )
                    .or_else(&[TxnOp::get("txn01", None)][..]),
            )
            .await?;

        assert!(resp.succeeded());
        let op_responses = resp.op_responses();
        assert_eq!(op_responses.len(), 1);

        match op_responses[0] {
            TxnOpResponse::Put(ref resp) => assert_eq!(resp.prev_key().unwrap().value(), b"01"),
            _ => panic!("unexpected response"),
        }

        let resp = client.get("txn01", None).await?;
        assert_eq!(resp.kvs()[0].key(), b"txn01");
        assert_eq!(resp.kvs()[0].value(), b"02");
    }

    // transaction 2
    {
        let resp = client
            .txn(
                Txn::new()
                    .when(&[Compare::value("txn01", CompareOp::Equal, "01")][..])
                    .and_then(&[TxnOp::put("txn01", "02", None)][..])
                    .or_else(&[TxnOp::get("txn01", None)][..]),
            )
            .await?;

        assert!(!resp.succeeded());
        let op_responses = resp.op_responses();
        assert_eq!(op_responses.len(), 1);

        match op_responses[0] {
            TxnOpResponse::Get(ref resp) => assert_eq!(resp.kvs()[0].value(), b"02"),
            _ => panic!("unexpected response"),
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_watch() -> Result<()> {
    let mut client = get_client().await?;

    let (mut watcher, mut stream) = client.watch("watch01", None).await?;

    client.put("watch01", "01", None).await?;

    let resp = stream.message().await?.unwrap();
    assert_eq!(resp.watch_id(), watcher.watch_id());
    assert_eq!(resp.events().len(), 1);

    let kv = resp.events()[0].kv().unwrap();
    assert_eq!(kv.key(), b"watch01");
    assert_eq!(kv.value(), b"01");
    assert_eq!(resp.events()[0].event_type(), EventType::Put);

    watcher.cancel().await?;

    let resp = stream.message().await?.unwrap();
    assert_eq!(resp.watch_id(), watcher.watch_id());
    assert!(resp.canceled());

    Ok(())
}

#[tokio::test]
async fn test_grant_revoke() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.lease_grant(123, None).await?;
    assert_eq!(resp.ttl(), 123);
    let id = resp.id();
    client.lease_revoke(id).await?;
    Ok(())
}

#[tokio::test]
async fn test_keep_alive() -> Result<()> {
    let mut client = get_client().await?;

    let resp = client.lease_grant(60, None).await?;
    assert_eq!(resp.ttl(), 60);
    let id = resp.id();

    let (mut keeper, mut stream) = client.lease_keep_alive(id).await?;
    keeper.keep_alive().await?;

    let resp = stream.message().await?.unwrap();
    assert_eq!(resp.id(), keeper.id());
    assert_eq!(resp.ttl(), 60);

    client.lease_revoke(id).await?;

    // keep alive a revoked lease should return error
    assert!(client.lease_keep_alive(id).await.is_err());
    Ok(())
}

#[tokio::test]
async fn test_time_to_live() -> Result<()> {
    let mut client = get_client().await?;
    let lease_id = 200;
    let resp = client
        .lease_grant(60, Some(LeaseGrantOptions::new().with_id(lease_id)))
        .await?;
    assert_eq!(resp.ttl(), 60);
    assert_eq!(resp.id(), lease_id);

    let resp = client.lease_time_to_live(lease_id, None).await?;
    assert_eq!(resp.id(), lease_id);
    assert_eq!(resp.granted_ttl(), 60);

    client.lease_revoke(lease_id).await?;
    Ok(())
}

#[tokio::test]
async fn test_leases() -> Result<()> {
    let lease1 = 100;
    let lease2 = 101;
    let lease3 = 102;

    let mut client = get_client().await?;
    let resp = client
        .lease_grant(60, Some(LeaseGrantOptions::new().with_id(lease1)))
        .await?;
    assert_eq!(resp.ttl(), 60);
    assert_eq!(resp.id(), lease1);

    let resp = client
        .lease_grant(60, Some(LeaseGrantOptions::new().with_id(lease2)))
        .await?;
    assert_eq!(resp.ttl(), 60);
    assert_eq!(resp.id(), lease2);

    let resp = client
        .lease_grant(60, Some(LeaseGrantOptions::new().with_id(lease3)))
        .await?;
    assert_eq!(resp.ttl(), 60);
    assert_eq!(resp.id(), lease3);

    let resp = client.leases().await?;
    let leases: Vec<_> = resp.leases().iter().map(|status| status.id()).collect();
    assert!(leases.contains(&lease1));
    assert!(leases.contains(&lease2));
    assert!(leases.contains(&lease3));

    client.lease_revoke(lease1).await?;
    client.lease_revoke(lease2).await?;
    client.lease_revoke(lease3).await?;
    Ok(())
}

#[tokio::test]
async fn test_lock() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.lock("lock-test", None).await?;
    let key = resp.key();
    let key_str = std::str::from_utf8(key)?;
    assert!(key_str.starts_with("lock-test/"));

    client.unlock(key).await?;
    Ok(())
}

#[ignore]
#[tokio::test]
async fn test_auth() -> Result<()> {
    let mut client = get_client().await?;
    client.auth_enable().await?;

    // after enable auth, must operate by authenticated client
    client.put("auth-test", "value", None).await.unwrap_err();

    // connect with authenticate, the user must already exists
    let options = Some(ConnectOptions::new().with_user(
        "root",    // user name
        "rootpwd", // password
    ));
    let mut client_auth = Client::connect(["localhost:2379"], options).await?;
    client_auth.put("auth-test", "value", None).await?;

    client_auth.auth_disable().await?;

    // after disable auth, operate ok
    let mut client = get_client().await?;
    client.put("auth-test", "value", None).await?;

    Ok(())
}

#[tokio::test]
async fn test_role() -> Result<()> {
    let mut client = get_client().await?;

    let role1 = "role1";
    let role2 = "role2";

    let _ = client.role_delete(role1).await;
    let _ = client.role_delete(role2).await;

    client.role_add(role1).await?;

    client.role_get(role1).await?;

    client.role_delete(role1).await?;
    client.role_get(role1).await.unwrap_err();

    client.role_add(role2).await?;
    client.role_get(role2).await?;

    {
        let resp = client.role_list().await?;
        assert!(resp.roles().contains(&role2.to_string()));
    }

    client
        .role_grant_permission(role2, Permission::read("123"))
        .await?;
    client
        .role_grant_permission(role2, Permission::write("abc").with_from_key())
        .await?;
    client
        .role_grant_permission(role2, Permission::read_write("hi").with_range_end("hjj"))
        .await?;
    client
        .role_grant_permission(
            role2,
            Permission::new(PermissionType::Write, "pp").with_prefix(),
        )
        .await?;
    client
        .role_grant_permission(
            role2,
            Permission::new(PermissionType::Read, "xyz").with_all_keys(),
        )
        .await?;

    {
        let resp = client.role_get(role2).await?;
        let permissions = resp.permissions();
        assert!(permissions.contains(&Permission::read("123")));
        assert!(permissions.contains(&Permission::write("abc").with_from_key()));
        assert!(permissions.contains(&Permission::read_write("hi").with_range_end("hjj")));
        assert!(permissions.contains(&Permission::write("pp").with_prefix()));
        assert!(permissions.contains(&Permission::read("xyz").with_all_keys()));
    }

    //revoke all permission
    client.role_revoke_permission(role2, "123", None).await?;
    client
        .role_revoke_permission(
            role2,
            "abc",
            Some(RoleRevokePermissionOptions::new().with_from_key()),
        )
        .await?;
    client
        .role_revoke_permission(
            role2,
            "hi",
            Some(RoleRevokePermissionOptions::new().with_range_end("hjj")),
        )
        .await?;
    client
        .role_revoke_permission(
            role2,
            "pp",
            Some(RoleRevokePermissionOptions::new().with_prefix()),
        )
        .await?;
    client
        .role_revoke_permission(
            role2,
            "xyz",
            Some(RoleRevokePermissionOptions::new().with_all_keys()),
        )
        .await?;

    let resp = client.role_get(role2).await?;
    assert!(resp.permissions().is_empty());

    client.role_delete(role2).await?;

    Ok(())
}

#[tokio::test]
async fn test_user() -> Result<()> {
    let name1 = "usr1";
    let password1 = "pwd1";
    let name2 = "usr2";
    let password2 = "pwd2";
    let name3 = "usr3";
    let password3 = "pwd3";
    let role1 = "role1";

    let mut client = get_client().await?;

    // ignore result
    let _resp = client.user_delete(name1).await;
    let _resp = client.user_delete(name2).await;
    let _resp = client.user_delete(name3).await;
    let _resp = client.role_delete(role1).await;

    client
        .user_add(name1, password1, Some(UserAddOptions::new()))
        .await?;

    client
        .user_add(name2, password2, Some(UserAddOptions::new().with_no_pwd()))
        .await?;

    client.user_add(name3, password3, None).await?;

    client.user_get(name1).await?;

    {
        let resp = client.user_list().await?;
        assert!(resp.users().contains(&name1.to_string()));
    }

    client.user_delete(name2).await?;
    client.user_get(name2).await.unwrap_err();

    client.user_change_password(name1, password2).await?;
    client.user_get(name1).await?;

    client.role_add(role1).await?;
    client.user_grant_role(name1, role1).await?;
    client.user_get(name1).await?;

    client.user_revoke_role(name1, role1).await?;
    client.user_get(name1).await?;

    let _ = client.user_delete(name1).await;
    let _ = client.user_delete(name2).await;
    let _ = client.user_delete(name3).await;
    let _ = client.role_delete(role1).await;

    Ok(())
}

#[tokio::test]
async fn test_alarm() -> Result<()> {
    let mut client = get_client().await?;

    // Test deactivate alarm.
    {
        let options = AlarmOptions::new();
        let _resp = client
            .alarm(AlarmAction::Deactivate, AlarmType::None, Some(options))
            .await?;
    }

    // Test get None alarm.
    let member_id = {
        let resp = client
            .alarm(AlarmAction::Get, AlarmType::None, None)
            .await?;
        let mems = resp.alarms();
        assert_eq!(mems.len(), 0);
        0
    };

    let mut options = AlarmOptions::new();
    options.with_member(member_id);

    // Test get no space alarm.
    {
        let resp = client
            .alarm(AlarmAction::Get, AlarmType::Nospace, Some(options.clone()))
            .await?;
        let mems = resp.alarms();
        assert_eq!(mems.len(), 0);
    }

    Ok(())
}

#[tokio::test]
async fn test_status() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.status().await?;

    let db_size = resp.db_size();
    assert_ne!(db_size, 0);
    Ok(())
}

#[tokio::test]
async fn test_defragment() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.defragment().await?;
    let hd = resp.header();
    assert!(hd.is_none());
    Ok(())
}

#[tokio::test]
async fn test_hash() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.hash().await?;
    let hd = resp.header();
    assert!(hd.is_some());
    assert_ne!(resp.hash(), 0);
    Ok(())
}

#[tokio::test]
async fn test_hash_kv() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.hash_kv(0).await?;
    let hd = resp.header();
    assert!(hd.is_some());
    assert_ne!(resp.hash(), 0);
    assert_ne!(resp.compact_version(), 0);
    Ok(())
}

#[tokio::test]
async fn test_snapshot() -> Result<()> {
    let mut client = get_client().await?;
    let mut msg = client.snapshot().await?;
    loop {
        if let Some(resp) = msg.message().await? {
            assert!(!resp.blob().is_empty());
            if resp.remaining_bytes() == 0 {
                break;
            }
        }
    }
    Ok(())
}

#[ignore]
#[tokio::test]
async fn test_cluster() -> Result<()> {
    let node1 = "localhost:2520";
    let node2 = "localhost:2530";
    let node3 = "localhost:2540";
    let mut client = get_client().await?;
    let resp = client
        .member_add([node1], Some(MemberAddOptions::new().with_is_learner()))
        .await?;
    let id1 = resp.member().unwrap().id();

    let resp = client.member_add([node2], None).await?;
    let id2 = resp.member().unwrap().id();
    let resp = client.member_add([node3], None).await?;
    let id3 = resp.member().unwrap().id();

    let resp = client.member_list().await?;
    let members: Vec<_> = resp.members().iter().map(|member| member.id()).collect();
    assert!(members.contains(&id1));
    assert!(members.contains(&id2));
    assert!(members.contains(&id3));
    Ok(())
}

#[tokio::test]
async fn test_move_leader() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.member_list().await?;
    let member_list = resp.members();

    let resp = client.status().await?;
    let leader_id = resp.leader();
    println!("status {:?}, leader_id {:?}", resp, resp.leader());

    let mut member_id = leader_id;
    for member in member_list {
        println!("member_id {:?}, name is {:?}", member.id(), member.name());
        if member.id() != leader_id {
            member_id = member.id();
            break;
        }
    }

    let resp = client.move_leader(member_id).await?;
    let header = resp.header();
    if member_id == leader_id {
        assert!(header.is_none());
    } else {
        assert!(header.is_some());
    }

    Ok(())
}

#[tokio::test]
async fn test_election() -> Result<()> {
    let mut client = get_client().await?;
    let resp = client.lease_grant(10, None).await?;
    let lease_id = resp.id();
    assert_eq!(resp.ttl(), 10);

    let resp = client.campaign("myElection", "123", lease_id).await?;
    let leader = resp.leader().unwrap();
    assert_eq!(leader.name(), b"myElection");
    assert_eq!(leader.lease(), lease_id);

    let resp = client
        .proclaim(
            "123",
            Some(ProclaimOptions::new().with_leader(leader.clone())),
        )
        .await?;
    let header = resp.header();
    println!("proclaim header {:?}", header.unwrap());
    assert!(header.is_some());

    let mut msg = client.observe(leader.name()).await?;
    loop {
        if let Some(resp) = msg.message().await? {
            assert!(resp.kv().is_some());
            println!("observe key {:?}", resp.kv().unwrap().key_str());
            if resp.kv().is_some() {
                break;
            }
        }
    }

    let resp = client.leader("myElection").await?;
    let kv = resp.kv().unwrap();
    assert_eq!(kv.value(), b"123");
    assert_eq!(kv.key(), leader.key());
    println!("key is {:?}", kv.key_str());
    println!("value is {:?}", kv.value_str());

    let resign_option = ResignOptions::new().with_leader(leader.clone());

    let resp = client.resign(Some(resign_option)).await?;
    let header = resp.header();
    println!("resign header {:?}", header.unwrap());
    assert!(header.is_some());

    Ok(())
}

#[tokio::test]
async fn test_remove_and_add_endpoint() -> Result<()> {
    let mut client = get_client().await?;
    client.put("endpoint", "add_remove", None).await?;

    // get key
    {
        let resp = client.get("endpoint", None).await?;
        assert_eq!(resp.count(), 1);
        assert!(!resp.more());
        assert_eq!(resp.kvs().len(), 1);
        assert_eq!(resp.kvs()[0].key(), b"endpoint");
        assert_eq!(resp.kvs()[0].value(), b"add_remove");
    }

    // remove endpoint
    client.remove_endpoint(DEFAULT_TEST_ENDPOINT).await?;
    // `Client::get` will hang before adding the endpoint back
    client.add_endpoint(DEFAULT_TEST_ENDPOINT).await?;

    // get key after remove and add endpoint
    {
        let resp = client.get("endpoint", None).await?;
        assert_eq!(resp.count(), 1);
        assert!(!resp.more());
        assert_eq!(resp.kvs().len(), 1);
        assert_eq!(resp.kvs()[0].key(), b"endpoint");
        assert_eq!(resp.kvs()[0].value(), b"add_remove");
    }

    Ok(())
}
