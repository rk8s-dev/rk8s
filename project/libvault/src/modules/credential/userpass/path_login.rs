use std::{collections::HashMap, sync::Arc};

use super::{UserPassBackend, UserPassBackendInner};
use crate::{
    context::Context,
    errors::RvError,
    logical::{Auth, Backend, Field, FieldType, Lease, Operation, Path, PathOperation, Request, Response},
    new_fields, new_fields_internal, new_path, new_path_internal, rv_error_string,
    utils::policy::equivalent_policies,
};

impl UserPassBackend {
    pub fn login_path(&self) -> Path {
        let userpass_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"login/(?P<username>\w[\w-]+\w)",
            fields: {
                "username": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Username of the user."
                },
                "password": {
                    field_type: FieldType::SecretStr,
                    required: true,
                    description: "Password for this user."
                }
            },
            operations: [
                {op: Operation::Write, handler: userpass_backend_ref.login}
            ],
            help: r#"This endpoint authenticates using a username and password."#
        });

        path
    }
}

#[maybe_async::maybe_async]
impl UserPassBackendInner {
    pub async fn login(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let err_info = "invalid username or password";
        let username_value = req.get_data("username")?;
        let username = username_value.as_str().unwrap().to_lowercase();
        let password_value = req.get_data("password")?;
        let password = password_value.as_str().unwrap();

        let user = self.get_user(req, &username).await?;
        if user.is_none() {
            log::error!("{err_info}");
            let resp = Response::error_response(err_info);
            return Ok(Some(resp));
        }

        let user = user.unwrap();

        let check = self.verify_password_hash(password, &user.password_hash)?;
        if !check {
            log::error!("{err_info}");
            let resp = Response::error_response(err_info);
            return Ok(Some(resp));
        }

        let mut auth = Auth {
            lease: Lease {
                ttl: user.ttl,
                max_ttl: user.max_ttl,
                renewable: user.ttl.as_secs() > 0,
                ..Default::default()
            },
            display_name: username.to_string(),
            policies: user.policies.clone(),
            ..Default::default()
        };
        auth.metadata.insert("username".to_string(), username.to_string());

        user.populate_token_auth(&mut auth);

        let resp = Response { auth: Some(auth), ..Response::default() };

        Ok(Some(resp))
    }

    pub async fn login_renew(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        if req.auth.is_none() {
            return Err(rv_error_string!("invalid request"));
        }
        let mut auth = req.auth.clone().unwrap();
        let username = auth.metadata.get("username");
        if username.is_none() {
            return Ok(None);
        }
        let username = username.unwrap();

        let user = self.get_user(req, username.as_str()).await?;
        if user.is_none() {
            return Ok(None);
        }

        let user = user.unwrap();

        if !equivalent_policies(&user.policies, &auth.policies) {
            return Err(rv_error_string!("policies have changed, not renewing"));
        }

        auth.period = user.token_period;
        auth.ttl = user.token_ttl;
        auth.max_ttl = user.token_max_ttl;

        Ok(Some(Response { auth: Some(auth), ..Response::default() }))
    }
}
