use std::sync::Arc;

use clap::{Args, ValueEnum};
use dashmap::DashMap;
use derive_more::{Deref, Display};
use lazy_static::lazy_static;
use prettytable::{format::FormatBuilder, Cell, Row, Table};
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::{api::secret::Secret, errors::RvError, rv_error_string};

lazy_static! {
    static ref UNDERSCORE_REGEX: Regex = Regex::new(r"_(\w)").unwrap();
}

#[derive(Args)]
#[group(required = false, multiple = true)]
pub struct OutputOptions {
    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        num_args = 0..=1,
        env = "VAULT_FORMAT",
        default_value_t = Format::Table,
        default_missing_value = "table",
        long_help = r#"Print the output in the given format.  This can also be specified via the
VAULT_FORMAT environment variable."#,
        value_enum
    )]
    format: Format,
}

#[derive(Args, Deref)]
#[group(required = false, multiple = true)]
pub struct LogicalOutputOptions {
    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        long_help = r#"Print only the field with the given name. Specifying this option will take precedence
over other formatting directives. The result will not have a trailing newline making
it ideal for piping to other processes."#
    )]
    pub field: Option<String>,

    #[deref]
    #[command(flatten)]
    pub output: OutputOptions,
}

#[derive(Display, ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    #[display(fmt = "table")]
    Table,

    #[display(fmt = "json")]
    Json,

    #[display(fmt = "yaml")]
    Yaml,

    #[display(fmt = "yml")]
    Yml,

    #[display(fmt = "raw")]
    Raw,
}

pub trait Formatter: Send + Sync {
    fn output(&self, data: &Value, _secret: Option<Secret>) -> Result<(), RvError> {
        let b = self.format(data)?;
        println!("{}", String::from_utf8_lossy(&b));
        Ok(())
    }
    fn format(&self, data: &Value) -> Result<Vec<u8>, RvError>;
}

lazy_static! {
    static ref Formatters: DashMap<String, Arc<dyn Formatter>> = {
        let map: DashMap<String, Arc<dyn Formatter>> = DashMap::new();

        map.insert("json".into(), Arc::new(JsonFormatter {}));
        map.insert("yaml".into(), Arc::new(YamlFormatter {}));
        map.insert("yml".into(), Arc::new(YamlFormatter {}));
        map.insert("table".into(), Arc::new(TableFormatter {}));
        map.insert("raw".into(), Arc::new(RawFormatter {}));

        map
    };
}

pub struct JsonFormatter;

impl Formatter for JsonFormatter {
    fn output(&self, data: &Value, secret: Option<Secret>) -> Result<(), RvError> {
        let b = self.format(data)?;

        if let Some(_s) = secret {
            //TODO
        }

        println!("{}", String::from_utf8_lossy(&b));
        Ok(())
    }

    fn format(&self, data: &Value) -> Result<Vec<u8>, RvError> {
        Ok(serde_json::to_string_pretty(data)?.as_bytes().to_vec())
    }
}

pub struct YamlFormatter;

impl Formatter for YamlFormatter {
    fn output(&self, data: &Value, _secret: Option<Secret>) -> Result<(), RvError> {
        let b = self.format(data)?;
        print!("{}", String::from_utf8_lossy(&b));
        Ok(())
    }

    fn format(&self, data: &Value) -> Result<Vec<u8>, RvError> {
        Ok(serde_yaml::to_string(data)?.as_bytes().to_vec())
    }
}

pub struct RawFormatter;

impl Formatter for RawFormatter {
    fn format(&self, data: &Value) -> Result<Vec<u8>, RvError> {
        Ok(serde_json::to_string(data)?.as_bytes().to_vec())
    }
}

pub fn convert_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut new_map = Map::new();
            for (key, value) in map {
                let new_key = UNDERSCORE_REGEX
                    .replace_all(&key.to_string(), |caps: &regex::Captures| {
                        let captured_char = caps.get(1).unwrap().as_str();
                        format!(" {}", captured_char.to_ascii_uppercase())
                    })
                    .trim_start()
                    .to_string()
                    .chars()
                    .enumerate()
                    .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
                    .collect::<String>();
                new_map.insert(new_key, convert_keys(value));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            let mut new_arr = Vec::new();
            for item in arr {
                new_arr.push(convert_keys(item));
            }
            Value::Array(new_arr)
        }
        _ => value.clone(),
    }
}

static SEPS: [&str; 15] = [
    "",
    "-",
    "--",
    "---",
    "----",
    "-----",
    "------",
    "-------",
    "--------",
    "---------",
    "----------",
    "-----------",
    "------------",
    "-------------",
    "--------------",
];

pub fn table_data_add_header(data: &Value, headers: &[&str]) -> Result<Value, RvError> {
    let mut array: Value = json!([]);
    if let Value::Array(ref mut arr) = array {
        if data.is_object() {
            if headers.len() != 2 {
                return Err(rv_error_string!("table_data_add_header failed: headers.len() != 2"));
            }
            arr.push(json!([headers[0], headers[1]]));
            arr.push(json!([SEPS[headers[0].len().min(SEPS.len())], SEPS[headers[1].len().min(SEPS.len())]]));
            let data_obj = data.as_object().unwrap();
            for (k, v) in data_obj.iter() {
                arr.push(json!([k, v.clone()]));
            }
        } else if data.is_array() {
            let data_arr = data.as_array().unwrap();

            let mut title: Value = json!([]);
            let mut sep: Value = json!([]);
            let title_arr = title.as_array_mut().unwrap();
            let sep_arr = sep.as_array_mut().unwrap();

            for h in headers.iter() {
                title_arr.push(json!(h));
                sep_arr.push(json!(SEPS[h.len().min(SEPS.len())]));
            }

            arr.push(title);
            arr.push(sep);

            for item in data_arr.iter() {
                if item.is_array() {
                    if item.as_array().unwrap().len() != headers.len() {
                        return Err(rv_error_string!("table_data_add_header failed: headers.len() != data[i].len()"));
                    }
                    arr.push(item.clone());
                } else if item.is_object() {
                    if headers.len() != 2 {
                        return Err(rv_error_string!("table_data_add_header failed: headers.len() != 2"));
                    }

                    let data_obj = item.as_object().unwrap();
                    for (k, v) in data_obj.iter() {
                        arr.push(json!([k, v.clone()]));
                    }
                } else {
                    if headers.len() != 1 {
                        return Err(rv_error_string!("table_data_add_header failed: headers.len() != 1"));
                    }
                    arr.push(item.clone());
                }
            }
        }
    }

    Ok(array)
}

pub struct TableFormatter;

impl Formatter for TableFormatter {
    fn output(&self, data: &Value, secret: Option<Secret>) -> Result<(), RvError> {
        if let Some(_s) = secret {
            //TODO
        }

        let b = self.format(data)?;

        print!("{}", String::from_utf8_lossy(&b));
        Ok(())
    }

    fn format(&self, data: &Value) -> Result<Vec<u8>, RvError> {
        if data.is_string() {
            return Ok(serde_yaml::to_string(data)?.as_bytes().to_vec());
        }

        let mut table = Table::new();

        if data.is_array() {
            let rows = data.as_array().unwrap();
            for row in rows.iter() {
                if row.is_array() {
                    let cells = row.as_array().unwrap();
                    let c = cells
                        .iter()
                        .map(|i| -> Cell {
                            Cell::new(
                                i.as_str()
                                    .map_or(i.to_string().as_str(), |s| if s.is_empty() { "n/a" } else { s })
                                    .trim(),
                            )
                        })
                        .collect();
                    table.add_row(Row::new(c));
                } else if row.is_object() {
                    let objs = row.as_object().unwrap();
                    for (k, v) in objs.iter() {
                        table.add_row(Row::new(vec![
                            Cell::new(k),
                            Cell::new(
                                v.as_str()
                                    .map_or(v.to_string().as_str(), |s| if s.is_empty() { "n/a" } else { s })
                                    .trim(),
                            ),
                        ]));
                    }
                } else {
                    table.add_row(Row::new(vec![Cell::new(
                        row.as_str().map_or(row.to_string().as_str(), |s| if s.is_empty() { "n/a" } else { s }).trim(),
                    )]));
                }
            }
        } else if data.is_object() {
            let objs = data.as_object().unwrap();
            for (k, v) in objs.iter() {
                table.add_row(Row::new(vec![
                    Cell::new(k),
                    Cell::new(
                        v.as_str().map_or(v.to_string().as_str(), |s| if s.is_empty() { "n/a" } else { s }).trim(),
                    ),
                ]));
            }
        }

        table.set_format(FormatBuilder::new().padding(0, 4).build());

        let ret = table.to_string();

        Ok(ret.as_bytes().to_vec())
    }
}

impl OutputOptions {
    pub fn is_format_table(&self) -> bool {
        self.format == Format::Table
    }

    pub fn print_value(&self, value: &Value, title_casing: bool) -> Result<(), RvError> {
        let fm = self.format.to_string();
        let formater = Formatters.get(&fm).ok_or(RvError::ErrString(format!("Invalid output format: {fm}")))?;
        let data = if self.format == Format::Table && title_casing { &convert_keys(value) } else { value };

        formater.output(data, None)
    }

    pub fn print_keys(&self, value: &Value) -> Result<(), RvError> {
        if !value.is_array() {
            return Err(RvError::ErrRequestNoData);
        }

        let fm = self.format.to_string();
        let formater = Formatters.get(&fm).ok_or(RvError::ErrString(format!("Invalid output format: {fm}")))?;

        let data = if fm == "table" { &table_data_add_header(value, &["Keys"])? } else { value };

        formater.output(data, None)
    }

    pub fn print_data(&self, value: &Value, field: Option<&str>) -> Result<(), RvError> {
        let fm = self.format.to_string();
        let formater = Formatters.get(&fm).ok_or(RvError::ErrString(format!("Invalid output format: {fm}")))?;

        let map = value["data"].as_object().unwrap();

        let data = if let Some(key) = field {
            if let Some(item) = map.get(key) {
                if !item.is_string() {
                    return Err(rv_error_string!(format!(r#"Field "{key}" not present in secret"#)));
                }
                let secret = item.as_str().unwrap();
                Value::String(secret.to_string())
            } else {
                return Err(rv_error_string!(format!(r#"Field "{key}" not present in secret"#)));
            }
        } else if self.format == Format::Table {
            table_data_add_header(&Value::Object(map.clone()), &["Key", "Value"])?
        } else {
            Value::Object(map.clone())
        };

        formater.output(&data, None)
    }

    pub fn print_secret(&self, secret: &Secret, _field: Option<&str>) -> Result<(), RvError> {
        let fm = self.format.to_string();
        let formater = Formatters.get(&fm).ok_or(RvError::ErrString(format!("Invalid output format: {fm}")))?;

        let value = if fm == "table" && secret.auth.is_some() {
            let auth = secret.auth.as_ref().unwrap();
            let mut v = serde_json::json!({
                "token": auth.client_token.clone(),
                "token_accessor": auth.accessor.clone(),
                "token_duration": auth.lease_duration,
                "token_renewable": auth.renewable,
                "token_policies": auth.token_policies.clone(),
                "policies": auth.policies.clone(),
            })
            .as_object()
            .unwrap()
            .clone();
            for (key, val) in auth.metadata.iter() {
                v.insert(format!("token_meta_{key}").to_string(), Value::String(val.clone()));
            }
            let val = Value::Object(v);
            table_data_add_header(&val, &["Key", "Value"])?
        } else {
            serde_json::to_value(secret)?
        };

        formater.output(&value, None)
    }

    pub fn print_field(&self, _value: &Value, _field: &str) -> Result<(), RvError> {
        Ok(())
    }
}
