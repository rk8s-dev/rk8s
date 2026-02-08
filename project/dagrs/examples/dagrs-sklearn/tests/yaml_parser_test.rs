use std::collections::HashMap;

use dagrs_sklearn::{yaml_parser::YamlParser, Parser};

#[test]
fn file_not_found_test() {
    let no_such_file = YamlParser.parse_tasks("./no_such_file.yaml", HashMap::new());
    assert!(no_such_file.is_err())
}

#[test]
fn illegal_yaml_content() {
    let illegal_content =
        YamlParser.parse_tasks("tests/config/illegal_content.yaml", HashMap::new());
    assert!(illegal_content.is_err())
}

#[test]
fn empty_content() {
    let empty_content = YamlParser.parse_tasks("tests/config/empty_file.yaml", HashMap::new());

    assert!(empty_content.is_err())
}

#[test]
fn yaml_no_start_with_dagrs() {
    let forget_dagrs =
        YamlParser.parse_tasks("tests/config/no_start_with_dagrs.yaml", HashMap::new());
    assert!(forget_dagrs.is_err())
}

#[test]
fn yaml_task_no_name() {
    let no_task_name = YamlParser.parse_tasks("tests/config/no_task_name.yaml", HashMap::new());
    assert!(no_task_name.is_err())
}

#[test]
fn yaml_task_not_found_precursor() {
    let not_found_pre =
        YamlParser.parse_tasks("tests/config/precursor_not_found.yaml", HashMap::new());
    assert!(not_found_pre.is_err())
}

#[test]
fn yaml_task_no_script_config() {
    let script = YamlParser.parse_tasks("tests/config/no_script.yaml", HashMap::new());
    assert!(script.is_err())
}

#[test]
fn correct_parse() {
    let tasks = YamlParser.parse_tasks("tests/config/correct.yaml", HashMap::new());
    assert!(tasks.is_ok());
}
