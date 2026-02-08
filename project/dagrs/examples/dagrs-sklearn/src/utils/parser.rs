//! Task configuration file parser interface
use std::collections::HashMap;

use dagrs::{Action, EnvVar, Graph};

/// Generic parser traits. If users want to customize the configuration file parser, they must implement this trait.
/// The yaml module's `YamlParser` is an example.
pub trait Parser {
    /// Parses the contents of a configuration file into a series of tasks with dependencies.
    /// Parameter Description:
    /// - file: path information of the configuration file
    /// - specific_actions: When parsing the configuration file, the specific execution logic
    ///   of some tasks does not need to be specified in the configuration file, but is given
    ///   through this map. In the map's key-value pair, the key represents the unique identifier
    ///   of the task in the task's configuration file, and the value represents the execution
    ///   logic given by the user.
    ///
    /// Return value description:
    /// If an error is encountered during the parsing process, the return result is ParserError.
    /// Instead, return a series of concrete types that implement the [`Task`] trait.
    /// This may involve user-defined [`Task`], you can refer to `YamlTask` under the yaml module.
    #[allow(unused)]
    fn parse_tasks(
        &self,
        file: &str,
        specific_actions: HashMap<String, Box<dyn Action>>,
    ) -> Result<(Graph, EnvVar), ParseError>;

    fn parse_tasks_from_str(
        &self,
        content: &str,
        specific_actions: HashMap<String, Box<dyn Action>>,
    ) -> Result<(Graph, EnvVar), ParseError>;
}

#[allow(unused)]
#[derive(Debug)]
pub struct ParseError(pub String);
