//! Some tests of the dag engine.

use std::collections::HashMap;

use dagrs::graph::error::GraphError;
use dagrs_sklearn::{yaml_parser::YamlParser, Parser};

fn runtime() -> dagrs::tokio::runtime::Runtime {
    dagrs::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
#[allow(deprecated)]
fn yaml_task_correct_execute() {
    let (mut job, _) = YamlParser
        .parse_tasks("tests/config/correct.yaml", HashMap::new())
        .unwrap();
    let runtime = runtime();
    job.start_with_runtime(&runtime).unwrap();
}

#[test]
#[allow(deprecated)]
fn yaml_task_loop_graph() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/loop_error.yaml", HashMap::new())
        .unwrap();
    let runtime = runtime();

    let res = res.start_with_runtime(&runtime);
    assert!(matches!(res, Err(GraphError::GraphLoopDetected)))
}

#[test]
#[allow(deprecated)]
fn yaml_task_self_loop_graph() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/self_loop_error.yaml", HashMap::new())
        .unwrap();
    let runtime = runtime();
    let res = res.start_with_runtime(&runtime);
    assert!(matches!(res, Err(GraphError::GraphLoopDetected)))
}

#[test]
#[allow(deprecated)]
fn yaml_task_failed_execute() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/script_run_failed.yaml", HashMap::new())
        .unwrap();
    let runtime = runtime();
    let res = res.start_with_runtime(&runtime);
    assert!(!res.is_ok())
}
