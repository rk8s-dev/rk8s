//! Some tests of the dag engine.

use std::collections::HashMap;

use dagrs::ErrorCode;
use dagrs_sklearn::{yaml_parser::YamlParser, Parser};

#[tokio::test]
async fn yaml_task_correct_execute() {
    let (mut job, _) = YamlParser
        .parse_tasks("tests/config/correct.yaml", HashMap::new())
        .unwrap();
    job.async_start().await.unwrap();
}

#[tokio::test]
async fn yaml_task_loop_graph() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/loop_error.yaml", HashMap::new())
        .unwrap();
    let err = res.async_start().await.expect_err("loop graph should fail");
    assert_eq!(err.code, ErrorCode::DgBld0004GraphLoopDetected);
}

#[tokio::test]
async fn yaml_task_self_loop_graph() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/self_loop_error.yaml", HashMap::new())
        .unwrap();
    let err = res.async_start().await.expect_err("self-loop graph should fail");
    assert_eq!(err.code, ErrorCode::DgBld0004GraphLoopDetected);
}

#[tokio::test]
async fn yaml_task_failed_execute() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/script_run_failed.yaml", HashMap::new())
        .unwrap();
    let res = res.async_start().await;
    assert!(res.is_err())
}
