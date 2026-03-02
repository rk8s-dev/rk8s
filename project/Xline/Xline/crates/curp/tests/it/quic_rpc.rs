//! QUIC transport integration tests
//!
//! These tests verify the full QUIC RPC path: client → QuicChannel → QuicGrpcServer → Rpc.

use std::{sync::Arc, time::Duration};

use curp::{
    client::ClientApi,
    rpc::{FetchClusterRequest, FetchClusterResponse, MethodId, QuicChannel},
};
use curp_test_utils::{
    init_logger,
    test_cmd::{TestCommand, TestCommandResult},
};
use test_macros::abort_on_panic;

use crate::common::quic_group::QuicCurpGroup;

/// Test that fetch_cluster works over QUIC (unary RPC)
#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
#[serial_test::serial]
async fn quic_fetch_cluster() {
    init_logger();

    let group = QuicCurpGroup::new(3).await;

    let node = group.nodes.values().next().unwrap();
    let channel: QuicChannel =
        QuicChannel::connect_single_for_test(&node.addr, Arc::clone(&group.quic_client))
            .await
            .unwrap();

    let resp: FetchClusterResponse = channel
        .unary_call(
            MethodId::FetchCluster,
            FetchClusterRequest::default(),
            vec![],
            Duration::from_secs(5),
        )
        .await
        .unwrap();

    assert_eq!(resp.members.len(), 3);
    group.close().await;
}

/// Test that leader election works and we can find the leader via QUIC
#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
#[serial_test::serial]
async fn quic_get_leader() {
    init_logger();

    let group = QuicCurpGroup::new(3).await;
    let (leader_id, term) = group.get_leader().await;
    assert!(term > 0 && leader_id > 0, "should find a leader");
    group.close().await;
}

/// Test basic propose over QUIC (server-streaming via propose_stream)
#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
#[serial_test::serial]
async fn quic_basic_propose() {
    init_logger();

    let group = QuicCurpGroup::new(3).await;
    let client = group.new_client().await;

    let result = client
        .propose(&TestCommand::new_put(vec![0], 0), None, true)
        .await
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(result, TestCommandResult::new(vec![], vec![]));

    let result = client
        .propose(&TestCommand::new_get(vec![0]), None, true)
        .await
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(result, TestCommandResult::new(vec![0], vec![1]));
    group.close().await;
}

/// Test synced propose over QUIC
#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
#[serial_test::serial]
async fn quic_synced_propose() {
    init_logger();

    let group = QuicCurpGroup::new(3).await;
    let client = group.new_client().await;
    let cmd = TestCommand::new_put(vec![0], 0);

    let (er, index) = client.propose(&cmd, None, false).await.unwrap().unwrap();
    assert_eq!(er, TestCommandResult::new(vec![], vec![]));
    assert_eq!(index.unwrap(), 1.into());
    group.close().await;
}

/// Test that an unknown method ID returns a structured error (not a disconnect)
#[cfg(feature = "quic-test")]
#[tokio::test(flavor = "multi_thread")]
#[abort_on_panic]
#[serial_test::serial]
async fn quic_unknown_method_returns_error() {
    use curp::rpc::CurpError;

    init_logger();

    let group = QuicCurpGroup::new(3).await;
    let node = group.nodes.values().next().unwrap();
    let channel = QuicChannel::connect_single_for_test(&node.addr, Arc::clone(&group.quic_client))
        .await
        .unwrap();

    let result: Result<FetchClusterResponse, _> = channel
        .raw_unary_call(0xFFFF, vec![], vec![], Duration::from_secs(5))
        .await;

    // Assert: check error variant + weak content constraint to confirm
    // we actually hit the unknown-method branch (not some other Internal error).
    match &result {
        Err(CurpError::Internal(msg)) => {
            assert!(
                msg.contains("unknown method id"),
                "expected 'unknown method id' in error message, got: {msg}"
            );
        }
        other => panic!("expected CurpError::Internal for unknown method, got: {other:?}"),
    }
    group.close().await;
}
