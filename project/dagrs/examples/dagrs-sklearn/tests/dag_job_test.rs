//! Some tests of the dag engine.

use std::collections::HashMap;

use dagrs::graph::error::GraphError;
use dagrs_sklearn::{yaml_parser::YamlParser, Parser};

#[test]
fn yaml_task_correct_execute() {
    let (mut job, _) = YamlParser
        .parse_tasks("tests/config/correct.yaml", HashMap::new())
        .unwrap();
    job.start().unwrap();
}

#[test]
fn yaml_task_loop_graph() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/loop_error.yaml", HashMap::new())
        .unwrap();

    let res = res.start();
    assert!(matches!(res, Err(GraphError::GraphLoopDetected)))
}

#[test]
fn yaml_task_self_loop_graph() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/self_loop_error.yaml", HashMap::new())
        .unwrap();
    let res = res.start();
    assert!(matches!(res, Err(GraphError::GraphLoopDetected)))
}

#[test]
fn yaml_task_failed_execute() {
    let (mut res, _) = YamlParser
        .parse_tasks("tests/config/script_run_failed.yaml", HashMap::new())
        .unwrap();
    let res = res.start();
    assert!(!res.is_ok())
}
