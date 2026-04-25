use super::job::{GcJobResult, JobManager, JobOutcome, JobState};
use super::protocol::{ControlRequest, ControlResponse};
use super::runtime::{InstanceRecord, RuntimeRegistry};
use super::server::{ControlHandler, ControlServer};
use async_trait::async_trait;
use tempfile::tempdir;

#[test]
fn protocol_roundtrip_preserves_gc_request() {
    let req = ControlRequest::RunGc { dry_run: true };
    let raw = serde_json::to_vec(&req).expect("serialize request");
    let decoded: ControlRequest = serde_json::from_slice(&raw).expect("deserialize request");

    assert_eq!(decoded, req);
}

#[tokio::test]
async fn runtime_registry_auto_selects_single_live_instance() {
    let dir = tempdir().expect("tempdir");
    let registry = RuntimeRegistry::new(dir.path().to_path_buf());

    let record = InstanceRecord::new(
        std::process::id(),
        "/mnt/slayer".to_string(),
        registry.socket_path(std::process::id()),
        chrono::Utc::now(),
    );

    registry.write_record(&record).await.expect("write record");

    let selected = registry
        .select_instance(None)
        .await
        .expect("select instance");

    assert_eq!(selected.mount_point, "/mnt/slayer");
    assert_eq!(selected.pid, std::process::id());
}

#[tokio::test]
async fn job_manager_tracks_gc_job_lifecycle() {
    let jobs = JobManager::default();
    let job_id = jobs.create_gc_job(true).await;

    let pending = jobs.get(&job_id).await.expect("pending job");
    assert_eq!(pending.state, JobState::Pending);
    assert_eq!(pending.detail.as_deref(), Some("dry-run"));

    jobs.mark_running(&job_id).await.expect("mark running");

    let running = jobs.get(&job_id).await.expect("running job");
    assert_eq!(running.state, JobState::Running);

    jobs.finish(
        &job_id,
        GcJobResult {
            dry_run: true,
            orphan_slice_count: 2,
            orphan_object_count: 4,
            deleted_object_count: 0,
            error_count: 0,
            detail: Some("dry-run".to_string()),
        },
    )
    .await
    .expect("finish job");

    let finished = jobs.get(&job_id).await.expect("finished job");
    assert_eq!(finished.state, JobState::Succeeded);

    match finished.outcome {
        Some(JobOutcome::Gc(result)) => {
            assert_eq!(result.orphan_slice_count, 2);
            assert_eq!(result.orphan_object_count, 4);
            assert_eq!(result.deleted_object_count, 0);
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

struct FakeHandler;

#[async_trait]
impl ControlHandler for FakeHandler {
    async fn handle(&self, request: ControlRequest) -> ControlResponse {
        match request {
            ControlRequest::Ping => ControlResponse::Pong,
            _ => ControlResponse::Error {
                code: "unsupported".to_string(),
                message: "unsupported".to_string(),
            },
        }
    }
}

#[tokio::test]
async fn uds_server_handles_single_request_response() {
    let dir = tempdir().expect("tempdir");
    let socket_path = dir.path().join("control.sock");
    let _server = ControlServer::bind(socket_path.clone(), FakeHandler)
        .await
        .expect("bind server");

    let response = super::client::send_request(&socket_path, &ControlRequest::Ping)
        .await
        .expect("send request");

    assert_eq!(response, ControlResponse::Pong);
}

#[tokio::test]
async fn uds_server_creates_parent_directory() {
    let dir = tempdir().expect("tempdir");
    let socket_path = dir.path().join("nested").join("control.sock");
    let _server = ControlServer::bind(socket_path.clone(), FakeHandler)
        .await
        .expect("bind server");

    assert!(socket_path.exists());
}
