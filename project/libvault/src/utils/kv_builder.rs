//! Builder is a struct to build a key/value mapping based on a list
//! of "k=v" pairs, where the value might come from stdin, a file, etc.

use std::{
    fs,
    io::{self, Read},
};

use serde_json::{Map, Value};

pub trait KvPairParse {
    fn to_map(&self) -> Map<String, Value>;
}

impl KvPairParse for Vec<String> {
    fn to_map(&self) -> Map<String, Value> {
        let mut map = Map::new();

        for entry in self {
            if let Some((key, value)) = entry.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim();

                let parsed_value = if value.starts_with('@') {
                    // Read from file
                    let file_path = value.strip_prefix('@').unwrap();
                    match fs::read_to_string(file_path) {
                        Ok(content) => Value::String(content),
                        Err(err) => {
                            eprintln!("Error reading file '{file_path}': {err}");
                            Value::Null
                        }
                    }
                } else if value.starts_with('-') {
                    // Read from stdin
                    let mut stdin_content = String::new();
                    match io::stdin().read_to_string(&mut stdin_content) {
                        Ok(_) => Value::String(stdin_content),
                        Err(err) => {
                            eprintln!("Error reading from stdin: {err}");
                            Value::Null
                        }
                    }
                } else {
                    // Direct value
                    Value::String(value.to_string())
                };

                map.insert(key, parsed_value);
            } else {
                eprintln!("Invalid key=value format: '{entry}'");
            }
        }

        map
    }
}

impl KvPairParse for &Vec<String> {
    fn to_map(&self) -> Map<String, Value> {
        // Leverage the implementation for Vec<String>
        (*self).to_map()
    }
}
