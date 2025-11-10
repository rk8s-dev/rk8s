use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    errors::RvError,
    logical::{Auth, Field, FieldType, FieldsBuilder, Request, field::FieldTrait},
    utils::{deserialize_duration, serialize_duration, sock_addr::SockAddrMarshaler},
};

// 24h
pub const DEFAULT_LEASE_TTL: Duration = Duration::from_secs(24 * 60 * 60_u64);
// 30d
pub const MAX_LEASE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60_u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenParams {
    #[serde(default)]
    pub token_type: String,

    // The TTL to user for the token
    #[serde(
        default,
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub token_ttl: Duration,

    // The max TTL to use for the token
    #[serde(
        default,
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub token_max_ttl: Duration,

    // If set, the token entry will have an explicit maximum TTL set, rather than deferring to role/mount values
    #[serde(
        default,
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub token_explicit_max_ttl: Duration,

    // If non-zero, tokens created using this role will be able to be renewed forever,
    // but will have a fixed renewal period of this value
    #[serde(
        default,
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub token_period: Duration,

    // If set, core will not automatically add default to the policy list
    #[serde(default)]
    pub token_no_default_policy: bool,

    // The maximum number of times a token issued from this role may be used.
    #[serde(default)]
    pub token_num_uses: u64,

    // The policies to set
    #[serde(default)]
    pub token_policies: Vec<String>,

    // The set of CIDRs that tokens generated using this role will be bound to
    #[serde(default)]
    pub token_bound_cidrs: Vec<SockAddrMarshaler>,
}

impl Default for TokenParams {
    fn default() -> Self {
        TokenParams {
            token_type: String::new(),
            token_ttl: Duration::from_secs(0),
            token_max_ttl: Duration::from_secs(0),
            token_explicit_max_ttl: Duration::from_secs(0),
            token_period: Duration::from_secs(0),
            token_no_default_policy: false,
            token_num_uses: 0,
            token_policies: Vec::new(),
            token_bound_cidrs: Vec::new(),
        }
    }
}

pub fn token_fields() -> HashMap<String, Arc<Field>> {
    FieldsBuilder::new()
        .field(
            "token_type",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("default")
                .description("The type of token to generate, service or batch"),
        )
        .field(
            "token_ttl",
            Field::builder()
                .field_type(FieldType::DurationSecond)
                .description("The initial ttl of the token to generate"),
        )
        .field(
            "token_max_ttl",
            Field::builder()
                .field_type(FieldType::DurationSecond)
                .description("The maximum lifetime of the generated token"),
        )
        .field(
            "token_explicit_max_ttl",
            Field::builder()
                .field_type(FieldType::DurationSecond)
                .description(
                    r#"If set, tokens created via this role carry an explicit maximum TTL.
During renewal, the current maximum TTL values of the role and the mount are not checked for changes,
and any updates to these values will have no effect on the token being renewed."#,
                ),
        )
        .field(
            "token_period",
            Field::builder()
                .field_type(FieldType::DurationSecond)
                .description(
                    r#"If set, tokens created via this role will have no max lifetime;
instead, their renewal period will be fixed to this value.  This takes an integer number of seconds,
or a string duration (e.g. "24h")."#,
                ),
        )
        .field(
            "token_no_default_policy",
            Field::builder()
                .field_type(FieldType::Bool)
                .description(
                    "If true, the 'default' policy will not automatically be added to generated tokens",
                ),
        )
        .field(
            "token_policies",
            Field::builder()
                .field_type(FieldType::CommaStringSlice)
                .description("Comma-separated list of policies"),
        )
        .field(
            "token_bound_cidrs",
            Field::builder()
                .field_type(FieldType::CommaStringSlice)
                .required(false)
                .description(
                    r#"Comma separated string or JSON list of CIDR blocks. If set, specifies the blocks of IP addresses which are allowed to use the generated token."#,
                ),
        )
        .field(
            "token_num_uses",
            Field::builder()
                .field_type(FieldType::Int)
                .description(
                    "The maximum number of times a token may be used, a value of zero means unlimited",
                ),
        )
        .build()
}

impl TokenParams {
    pub fn new(token_type: &str) -> Self {
        Self {
            token_type: token_type.to_string(),
            ..TokenParams::default()
        }
    }

    pub fn parse_token_fields(&mut self, req: &Request) -> Result<(), RvError> {
        if let Ok(ttl_value) = req.get_data("token_ttl") {
            self.token_ttl = ttl_value
                .as_duration()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(max_ttl_value) = req.get_data("token_max_ttl") {
            self.token_max_ttl = max_ttl_value
                .as_duration()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(explicit_max_ttl_value) = req.get_data("token_explicit_max_ttl") {
            self.token_explicit_max_ttl = explicit_max_ttl_value
                .as_duration()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(period_value) = req.get_data("token_period") {
            self.token_period = period_value
                .as_duration()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(no_default_policy_value) = req.get_data("token_no_default_policy") {
            self.token_no_default_policy = no_default_policy_value
                .as_bool()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(num_uses_value) = req.get_data("token_num_uses") {
            self.token_num_uses = num_uses_value
                .as_u64()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(type_value) = req.get_data_or_default("token_type") {
            let token_type = type_value
                .as_str()
                .ok_or(RvError::ErrRequestFieldInvalid)?
                .to_string();
            self.token_type = match token_type.as_str() {
                "" => "default".to_string(),
                "default-service" => "service".to_string(),
                "default-batch" => "batch".to_string(),
                _ => token_type.clone(),
            };

            match self.token_type.as_str() {
                "default" | "service" | "batch" => {}
                _ => {
                    return Err(RvError::ErrRequestFieldInvalid);
                }
            };
        }

        if let Ok(policies_value) = req.get_data("token_policies") {
            self.token_policies = policies_value
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(token_bound_cidrs_value) = req.get_data("token_bound_cidrs") {
            let token_bound_cidrs = token_bound_cidrs_value
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            self.token_bound_cidrs = token_bound_cidrs
                .iter()
                .map(|s| SockAddrMarshaler::from_str(s))
                .collect::<Result<Vec<SockAddrMarshaler>, _>>()?;
        }

        Ok(())
    }

    pub fn populate_token_data(&self, data: &mut Map<String, Value>) {
        data.insert("token_type".to_string(), json!(self.token_type.clone()));
        data.insert("token_ttl".to_string(), json!(self.token_ttl.as_secs()));
        data.insert(
            "token_max_ttl".to_string(),
            json!(self.token_max_ttl.as_secs()),
        );
        data.insert(
            "token_explicit_max_ttl".to_string(),
            json!(self.token_explicit_max_ttl.as_secs()),
        );
        data.insert(
            "token_period".to_string(),
            json!(self.token_period.as_secs()),
        );
        data.insert(
            "token_no_default_policy".to_string(),
            json!(self.token_no_default_policy),
        );
        data.insert("token_num_uses".to_string(), json!(self.token_num_uses));
        data.insert("token_policies".to_string(), json!(self.token_policies));
        data.insert(
            "token_bound_cidrs".to_string(),
            json!(self.token_bound_cidrs),
        );
    }

    pub fn populate_token_auth(&self, auth: &mut Auth) {
        auth.ttl = self.token_ttl;
        auth.max_ttl = self.token_max_ttl;
        auth.policies.clone_from(&self.token_policies);
        auth.no_default_policy = self.token_no_default_policy;
        auth.renewable = true;
    }
}
