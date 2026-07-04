use futures::executor::block_on;
use rk8s_oci::{ImageReference, Platform};
use rk8s_runtime_api::{
    ContainerId, ContainerRuntime, ContainerSource, ContainerState, ContainerStatus,
    ContainerStatusRequest, CreateContainerRequest, DeleteContainerRequest, ExecContainerRequest,
    ExecResult, ListContainersRequest, Result, RuntimeFeature, RuntimeStatus,
    StartContainerRequest, StopContainerRequest,
};

#[test]
fn create_request_captures_portable_runtime_contract() {
    let id = ContainerId::new("pod1-app").unwrap();
    let image: ImageReference = "alpine:3.20".parse().unwrap();
    let request = CreateContainerRequest::new(id.clone(), ContainerSource::image(image))
        .with_platform(Platform::linux_amd64())
        .with_arg("/bin/sh")
        .with_arg("-c")
        .with_arg("echo hi")
        .with_env("FOO", "bar")
        .with_annotation("io.rk8s.pod", "pod1");

    assert_eq!(request.id, id);
    assert_eq!(request.process.args, ["/bin/sh", "-c", "echo hi"]);
    assert_eq!(
        request.process.env.get("FOO").map(String::as_str),
        Some("bar")
    );
    assert_eq!(request.platform, Some(Platform::linux_amd64()));
    assert_eq!(
        request.annotations.get("io.rk8s.pod").map(String::as_str),
        Some("pod1")
    );

    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["source"]["type"], "image");
    assert_eq!(value["source"]["image"]["registry"], "docker.io");
    assert_eq!(value["source"]["image"]["repository"], "library/alpine");
}

#[test]
fn ids_reject_empty_or_path_like_values() {
    assert!(ContainerId::new("").is_err());
    assert!(ContainerId::new("../escape").is_err());
    assert!(ContainerId::new("pod/container").is_err());
    assert_eq!(
        ContainerId::new("pod_1.app-0").unwrap().as_str(),
        "pod_1.app-0"
    );
}

#[test]
fn runtime_trait_is_object_safe_for_adapters() {
    struct FakeRuntime;

    #[rk8s_runtime_api::async_trait]
    impl ContainerRuntime for FakeRuntime {
        fn name(&self) -> &str {
            "fake"
        }

        async fn status(&self) -> Result<RuntimeStatus> {
            Ok(RuntimeStatus::new("fake")
                .with_version("0.1.0")
                .with_feature(RuntimeFeature::Image)
                .healthy())
        }

        async fn create(&self, request: CreateContainerRequest) -> Result<ContainerStatus> {
            Ok(ContainerStatus::new(request.id, ContainerState::Created))
        }

        async fn start(&self, request: StartContainerRequest) -> Result<ContainerStatus> {
            Ok(ContainerStatus::new(request.id, ContainerState::Running))
        }

        async fn stop(&self, request: StopContainerRequest) -> Result<ContainerStatus> {
            Ok(ContainerStatus::new(request.id, ContainerState::Stopped))
        }

        async fn delete(&self, _request: DeleteContainerRequest) -> Result<()> {
            Ok(())
        }

        async fn exec(&self, _request: ExecContainerRequest) -> Result<ExecResult> {
            Ok(ExecResult::success().with_stdout(b"hello\n"))
        }

        async fn inspect(&self, request: ContainerStatusRequest) -> Result<ContainerStatus> {
            Ok(ContainerStatus::new(request.id, ContainerState::Running).with_pid(42))
        }

        async fn list(&self, _request: ListContainersRequest) -> Result<Vec<ContainerStatus>> {
            Ok(vec![ContainerStatus::new(
                ContainerId::new("demo").unwrap(),
                ContainerState::Running,
            )])
        }
    }

    let runtime: Box<dyn ContainerRuntime> = Box::new(FakeRuntime);
    let status = block_on(runtime.status()).unwrap();
    assert_eq!(runtime.name(), "fake");
    assert!(status.healthy);
    assert!(status.features.contains(&RuntimeFeature::Image));

    let id = ContainerId::new("demo").unwrap();
    let created = block_on(runtime.create(CreateContainerRequest::new(
        id.clone(),
        ContainerSource::oci_bundle("bundle"),
    )))
    .unwrap();
    assert_eq!(created.state, ContainerState::Created);

    let started = block_on(runtime.start(StartContainerRequest::new(id.clone()))).unwrap();
    assert_eq!(started.state, ContainerState::Running);

    let inspected = block_on(runtime.inspect(ContainerStatusRequest::new(id.clone()))).unwrap();
    assert_eq!(inspected.pid, Some(42));

    let exec = block_on(
        runtime.exec(
            ExecContainerRequest::new(id.clone())
                .with_arg("echo")
                .with_arg("hello"),
        ),
    )
    .unwrap();
    assert_eq!(exec.exit_code, 0);
    assert_eq!(exec.stdout, b"hello\n");

    block_on(runtime.delete(DeleteContainerRequest::new(id).force())).unwrap();
}
