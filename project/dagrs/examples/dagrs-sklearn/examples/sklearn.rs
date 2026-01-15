use std::{collections::HashMap, sync::Arc};

use dagrs::{async_trait::async_trait, Action, Content, EnvVar, InChannels, OutChannels, Output};
use dagrs_sklearn::{yaml_parser::YamlParser, CommandAction, Parser};

const ENV_DATA_SRC: &str = "data_src";

struct NodeAction {
    class: usize,
}

#[async_trait]
impl Action for NodeAction {
    async fn run(
        &self,
        _: &mut InChannels,
        out_channels: &mut OutChannels,
        env: Arc<EnvVar>,
    ) -> Output {
        let data_src: &&str = env.get_ref(ENV_DATA_SRC).unwrap();

        let cmd_act = CommandAction::new(
            "python",
            vec![
                "examples/lr_i.py".to_string(),
                format!("{}", data_src),
                format!("{}", self.class),
            ],
        );
        let result = cmd_act
            .run(&mut InChannels::default(), &mut OutChannels::default(), env)
            .await;
        match result {
            Output::Out(content) => {
                let content: Arc<(Vec<String>, Vec<String>)> =
                    content.unwrap().into_inner().unwrap();
                let (stdout, _) = (&content.0, &content.1);
                let theta = stdout.get(0).unwrap().clone();
                out_channels.broadcast(Content::new(theta)).await
            }
            Output::Err(e) => panic!("{}", e),
            Output::ErrWithExitCode(code, content) => panic!(
                "Exit with {:?}, {:?}",
                code,
                content.unwrap().get::<(Vec<String>, Vec<String>)>()
            ),
            _ => panic!(),
        };
        Output::empty()
    }
}

impl NodeAction {
    fn new(class: usize) -> Self {
        Self { class }
    }
}

struct RootAction;
#[async_trait]
impl Action for RootAction {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        _: &mut OutChannels,
        env: Arc<EnvVar>,
    ) -> Output {
        let data_src: &&str = env.get_ref(ENV_DATA_SRC).unwrap();
        let mut thetas: Vec<String> = in_channels
            .map(|theta| {
                let theta = theta.unwrap();
                let theta = theta.get::<String>().unwrap();
                theta.clone()
            })
            .await;

        let mut args = vec!["examples/lr_root.py".to_string(), format!("{}", data_src)];

        args.append(&mut thetas);

        let cmd_action = CommandAction::new("python", args);
        cmd_action
            .run(&mut InChannels::default(), &mut OutChannels::default(), env)
            .await
    }
}

fn main() {
    env_logger::init();

    let specific_actions: HashMap<String, Box<dyn Action>> = HashMap::from([
        (
            "node0".to_string(),
            Box::new(NodeAction::new(0)) as Box<dyn Action>,
        ),
        (
            "node1".to_string(),
            Box::new(NodeAction::new(1)) as Box<dyn Action>,
        ),
        (
            "node2".to_string(),
            Box::new(NodeAction::new(2)) as Box<dyn Action>,
        ),
        (
            "node3".to_string(),
            Box::new(NodeAction::new(3)) as Box<dyn Action>,
        ),
        (
            "node4".to_string(),
            Box::new(NodeAction::new(4)) as Box<dyn Action>,
        ),
        (
            "node5".to_string(),
            Box::new(NodeAction::new(5)) as Box<dyn Action>,
        ),
        (
            "node6".to_string(),
            Box::new(NodeAction::new(6)) as Box<dyn Action>,
        ),
        (
            "node7".to_string(),
            Box::new(NodeAction::new(7)) as Box<dyn Action>,
        ),
        (
            "node8".to_string(),
            Box::new(NodeAction::new(8)) as Box<dyn Action>,
        ),
        (
            "node9".to_string(),
            Box::new(NodeAction::new(9)) as Box<dyn Action>,
        ),
        ("root".to_string(), Box::new(RootAction) as Box<dyn Action>),
    ]);

    let (mut dag, mut env_var) = YamlParser
        .parse_tasks("examples/config.yml", specific_actions)
        .unwrap();
    env_var.set(ENV_DATA_SRC, "examples/ex3data1.mat");

    let root_id = *env_var.get_node_id("root").unwrap();
    dag.set_env(env_var);
    dag.start().unwrap();

    let outputs = dag.get_outputs();
    let result = outputs.get(&root_id).unwrap().get_out().unwrap();
    let (stdout, _) = result.get::<(Vec<String>, Vec<String>)>().unwrap();

    let acc = if cfg!(target_os = "windows") {
        stdout.get(1).unwrap()
    } else {
        stdout.get(0).unwrap()
    };
    assert_eq!("Accuracy: 94.46%", acc);

    println!("{}", acc)
}
