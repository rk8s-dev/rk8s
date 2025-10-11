use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

use enum_map::Enum;
use humantime::parse_duration;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use strum::{Display, EnumString};

use crate::errors::RvError;

#[derive(Eq, PartialEq, Copy, Clone, Debug, EnumString, Display, Enum, Serialize, Deserialize)]
pub enum FieldType {
    #[strum(to_string = "string")]
    Str,
    #[strum(to_string = "secret_string")]
    SecretStr,
    #[strum(to_string = "int")]
    Int,
    #[strum(to_string = "bool")]
    Bool,
    #[strum(to_string = "map")]
    Map,
    #[strum(to_string = "array")]
    Array,
    #[strum(to_string = "duration_second")]
    DurationSecond,
    #[strum(to_string = "comma_string_slice")]
    CommaStringSlice,
}

#[derive(Clone)]
pub struct Field {
    pub required: bool,
    pub field_type: FieldType,
    pub default: Value,
    pub description: String,
}

pub trait FieldTrait {
    fn is_bool_ex(&self) -> bool;
    fn is_int(&self) -> bool;
    fn is_duration(&self) -> bool;
    fn is_comma_string_slice(&self) -> bool;
    fn is_map(&self) -> bool;
    fn as_bool_ex(&self) -> Option<bool>;
    fn as_int(&self) -> Option<i64>;
    fn as_duration(&self) -> Option<Duration>;
    fn as_comma_string_slice(&self) -> Option<Vec<String>>;
    fn as_map(&self) -> Option<HashMap<String, String>>;
}

impl FieldTrait for Value {
    fn is_bool_ex(&self) -> bool {
        if self.is_boolean() {
            return true;
        }

        matches!(self.as_str(), Some("true") | Some("false"))
    }

    fn is_int(&self) -> bool {
        if self.is_i64() {
            return true;
        }

        let int_str = self.as_str();
        if int_str.is_none() {
            return false;
        }

        let int = int_str.unwrap().parse::<i64>().ok();
        if int.is_none() {
            return false;
        }

        true
    }

    fn is_duration(&self) -> bool {
        if self.is_i64() {
            return true;
        }

        if let Some(secs_str) = self.as_str()
            && (secs_str.parse::<u64>().ok().is_some() || parse_duration(secs_str).is_ok())
        {
            return true;
        }

        false
    }

    fn is_comma_string_slice(&self) -> bool {
        let arr = self.as_array();
        if arr.is_some() {
            let arr_val = arr.unwrap();
            for item in arr_val.iter() {
                let item_val = item.as_str();
                if item_val.is_some() {
                    continue;
                }

                let item_val = item.as_i64();
                if item_val.is_some() {
                    continue;
                }

                return false;
            }

            return true;
        }

        let value = self.as_i64();
        if value.is_some() {
            return true;
        }

        let value = self.as_str();
        if value.is_some() {
            return true;
        }

        false
    }

    fn is_map(&self) -> bool {
        if self.is_object() {
            return true;
        }

        let map_str = self.as_str();
        if map_str.is_none() {
            return false;
        }

        let map = serde_json::from_str::<Value>(map_str.unwrap());
        map.is_ok() && map.unwrap().is_object()
    }

    fn as_bool_ex(&self) -> Option<bool> {
        if self.is_boolean() {
            return self.as_bool();
        }

        match self.as_str() {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => None,
        }
    }

    fn as_int(&self) -> Option<i64> {
        let mut int = self.as_i64();
        if int.is_none() {
            let int_str = self.as_str();
            int_str?;

            int = int_str.unwrap().parse::<i64>().ok();
            int?;
        }

        int
    }

    fn as_duration(&self) -> Option<Duration> {
        if let Some(secs) = self.as_u64() {
            return Some(Duration::from_secs(secs));
        }

        if let Some(secs_str) = self.as_str() {
            if let Ok(secs_int) = secs_str.parse::<u64>() {
                return Some(Duration::from_secs(secs_int));
            } else if let Ok(ret) = parse_duration(secs_str) {
                return Some(ret);
            }
        }

        None
    }

    fn as_comma_string_slice(&self) -> Option<Vec<String>> {
        let mut ret = Vec::new();
        let arr = self.as_array();
        if arr.is_some() {
            let arr_val = arr.unwrap();
            for item in arr_val.iter() {
                let item_val = item.as_str();
                if item_val.is_some() {
                    ret.push(item_val.unwrap().trim().to_string());
                    continue;
                }

                let item_val = item.as_i64();
                if item_val.is_some() {
                    ret.push(item_val.unwrap().to_string());
                    continue;
                }

                return None;
            }

            return Some(ret);
        }

        let value = self.as_i64();
        if value.is_some() {
            ret.push(value.unwrap().to_string());
            return Some(ret);
        }

        let value = self.as_str();
        if value.is_some() {
            return Some(
                value
                    .unwrap()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            );
        }

        None
    }

    fn as_map(&self) -> Option<HashMap<String, String>> {
        let mut ret: HashMap<String, String> = HashMap::new();
        if let Some(map) = self.as_object() {
            for (key, value) in map {
                if !value.is_string() {
                    continue;
                }
                ret.insert(key.clone(), value.as_str().unwrap().to_string());
            }

            return Some(ret);
        }

        if let Some(value) = self.as_str() {
            let map: HashMap<String, String> =
                serde_json::from_str(value).unwrap_or(HashMap::new());
            return Some(map);
        }

        None
    }
}

impl Field {
    pub fn new() -> Self {
        Self {
            required: false,
            field_type: FieldType::Str,
            default: json!(null),
            description: String::new(),
        }
    }

    pub fn builder() -> FieldBuilder {
        FieldBuilder::new()
    }

    pub fn into_arc(self) -> Arc<Field> {
        Arc::new(self)
    }

    pub fn check_data_type(&self, data: &Value) -> bool {
        match &self.field_type {
            FieldType::SecretStr | FieldType::Str => data.is_string(),
            FieldType::Int => data.is_int(),
            FieldType::Bool => data.is_boolean(),
            FieldType::Array => data.is_array(),
            FieldType::Map => data.is_object(),
            FieldType::DurationSecond => data.is_duration(),
            FieldType::CommaStringSlice => data.is_comma_string_slice(),
        }
    }

    pub fn get_default(&self) -> Result<Value, RvError> {
        if self.default.is_null() {
            match &self.field_type {
                FieldType::SecretStr | FieldType::Str => {
                    return Ok(json!(""));
                }
                FieldType::Int => {
                    return Ok(json!(0));
                }
                FieldType::Bool => {
                    return Ok(json!(false));
                }
                FieldType::Array => {
                    return Ok(json!([]));
                }
                FieldType::Map => {
                    return Ok(serde_json::from_str("{}")?);
                }
                FieldType::DurationSecond => {
                    return Ok(json!(0));
                }
                FieldType::CommaStringSlice => {
                    return Ok(json!([]));
                }
            }
        }

        match &self.field_type {
            FieldType::SecretStr | FieldType::Str => {
                if self.default.is_string() {
                    return Ok(self.default.clone());
                }

                Err(RvError::ErrRustDowncastFailed)
            }
            FieldType::Int => {
                if self.default.is_i64() {
                    return Ok(self.default.clone());
                }

                Err(RvError::ErrRustDowncastFailed)
            }
            FieldType::Bool => {
                if self.default.is_boolean() {
                    return Ok(self.default.clone());
                }

                Err(RvError::ErrRustDowncastFailed)
            }
            FieldType::Array => {
                if self.default.is_array() {
                    return Ok(self.default.clone());
                } else if self.default.is_string() {
                    let arr_str = self.default.as_str();
                    if arr_str.is_none() {
                        return Err(RvError::ErrRustDowncastFailed);
                    }
                    return Ok(serde_json::from_str(arr_str.unwrap())?);
                }

                Err(RvError::ErrRustDowncastFailed)
            }
            FieldType::Map => {
                if self.default.is_object() {
                    return Ok(self.default.clone());
                } else if self.default.is_string() {
                    let arr_str = self.default.as_str();
                    if arr_str.is_none() {
                        return Err(RvError::ErrRustDowncastFailed);
                    }
                    return Ok(serde_json::from_str(arr_str.unwrap())?);
                }

                Err(RvError::ErrRustDowncastFailed)
            }
            FieldType::DurationSecond => {
                if self.default.is_duration() {
                    return Ok(self.default.clone());
                }

                Err(RvError::ErrRustDowncastFailed)
            }
            FieldType::CommaStringSlice => {
                if self.default.is_comma_string_slice() {
                    return Ok(self.default.clone());
                }

                Err(RvError::ErrRustDowncastFailed)
            }
        }
    }
}

#[derive(Clone)]
pub struct FieldBuilder {
    field: Field,
}

impl Default for FieldBuilder {
    fn default() -> Self {
        Self {
            field: Field::new(),
        }
    }
}

impl FieldBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn required(mut self, required: bool) -> Self {
        self.field.required = required;
        self
    }

    pub fn field_type(mut self, field_type: FieldType) -> Self {
        self.field.field_type = field_type;
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.field.description = description.into();
        self
    }

    pub fn default_value(mut self, default: impl Serialize) -> Self {
        self.field.default = serde_json::to_value(default).unwrap_or(Value::Null);
        self.field.required = false;
        self
    }

    pub fn build(self) -> Field {
        self.field
    }

    pub fn build_arc(self) -> Arc<Field> {
        Arc::new(self.field)
    }
}

pub trait IntoFieldArc {
    fn into_field_arc(self) -> Arc<Field>;
}

impl IntoFieldArc for Arc<Field> {
    fn into_field_arc(self) -> Arc<Field> {
        self
    }
}

impl IntoFieldArc for Field {
    fn into_field_arc(self) -> Arc<Field> {
        self.into_arc()
    }
}

impl IntoFieldArc for FieldBuilder {
    fn into_field_arc(self) -> Arc<Field> {
        self.build_arc()
    }
}

#[derive(Default, Clone)]
pub struct FieldsBuilder {
    fields: HashMap<String, Arc<Field>>,
}

impl FieldsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field<F>(mut self, name: impl Into<String>, field: F) -> Self
    where
        F: IntoFieldArc,
    {
        self.fields.insert(name.into(), field.into_field_arc());
        self
    }

    pub fn build(self) -> HashMap<String, Arc<Field>> {
        self.fields
    }
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Field")
            .field("required", &self.required)
            .field("field_type", &self.field_type)
            .field("default", &self.default)
            .field("description", &self.description)
            .finish()
    }
}
