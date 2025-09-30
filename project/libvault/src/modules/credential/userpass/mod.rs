use std::{any::Any, sync::Arc};

use derive_more::Deref;

use crate::{
    core::Core,
    errors::RvError,
    logical::{Backend, LogicalBackend},
    modules::{auth::AuthModule, Module},
    new_logical_backend, new_logical_backend_internal,
};

pub mod cli;
pub mod path_login;
pub mod path_users;

static USERPASS_BACKEND_HELP: &str = r#"
The "userpass" credential provider allows authentication using a combination of
a username and password. No additional factors are supported.

The username/password combination is configured using the "users/" endpoints by
a user with root access. Authentication is then done by supplying the two fields
for "login".
"#;

pub struct UserPassModule {
    pub name: String,
    pub backend: Arc<UserPassBackend>,
}

pub struct UserPassBackendInner {
    pub core: Arc<Core>,
}

#[derive(Deref)]
pub struct UserPassBackend {
    #[deref]
    pub inner: Arc<UserPassBackendInner>,
}

impl UserPassBackend {
    pub fn new(core: Arc<Core>) -> Self {
        Self { inner: Arc::new(UserPassBackendInner { core }) }
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let userpass_backend_ref = self.inner.clone();

        let mut backend = new_logical_backend!({
            unauth_paths: ["login/*"],
            auth_renew_handler: userpass_backend_ref.login_renew,
            help: USERPASS_BACKEND_HELP,
        });

        backend.paths.push(Arc::new(self.users_path()));
        backend.paths.push(Arc::new(self.user_list_path()));
        backend.paths.push(Arc::new(self.user_password_path()));
        backend.paths.push(Arc::new(self.login_path()));

        backend
    }
}

impl UserPassModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self { name: "userpass".to_string(), backend: Arc::new(UserPassBackend::new(core)) }
    }
}

impl Module for UserPassModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let userpass = self.backend.clone();
        let userpass_backend_new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut userpass_backend = userpass.new_backend();
            userpass_backend.init()?;
            Ok(Arc::new(userpass_backend))
        };

        if let Some(auth_module) = core.module_manager.get_module::<AuthModule>("auth") {
            return auth_module.add_auth_backend("userpass", Arc::new(userpass_backend_new_func));
        } else {
            log::error!("get auth module failed!");
        }

        Ok(())
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        if let Some(auth_module) = core.module_manager.get_module::<AuthModule>("auth") {
            return auth_module.delete_auth_backend("userpass");
        } else {
            log::error!("get auth module failed!");
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::{
        core::Core,
        logical::{Operation, Request, Response},
        test_utils::{
            new_unseal_test_rusty_vault, test_delete_api, test_mount_auth_api, test_read_api, test_write_api,
        },
    };

    #[maybe_async::maybe_async]
    async fn test_write_user(core: &Core, token: &str, path: &str, username: &str, password: &str, ttl: i32) {
        let user_data = json!({
            "password": password,
            "ttl": ttl,
        })
        .as_object()
        .cloned();

        let resp =
            test_write_api(core, token, format!("auth/{}/users/{}", path, username).as_str(), true, user_data).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn test_read_user(core: &Core, token: &str, username: &str) -> Result<Option<Response>, RvError> {
        let resp = test_read_api(core, token, format!("auth/pass/users/{}", username).as_str(), true).await;
        assert!(resp.is_ok());
        resp
    }

    #[maybe_async::maybe_async]
    async fn test_delete_user(core: &Core, token: &str, username: &str) {
        let resp = test_delete_api(core, token, format!("auth/pass/users/{}", username).as_str(), true, None).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn test_login(
        core: &Core,
        path: &str,
        username: &str,
        password: &str,
        is_ok: bool,
    ) -> Result<Option<Response>, RvError> {
        let login_data = json!({
            "password": password,
        })
        .as_object()
        .cloned();

        let mut req = Request::new(format!("auth/{}/login/{}", path, username).as_str());
        req.operation = Operation::Write;
        req.body = login_data;

        let resp = core.handle_request(&mut req).await;
        assert!(resp.is_ok());
        if is_ok {
            let resp = resp.as_ref().unwrap();
            assert!(resp.is_some());
        }
        resp
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_userpass_module() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_userpass_module").await;

        // mount userpass auth to path: auth/pass
        test_mount_auth_api(&core, &root_token, "userpass", "pass").await;

        test_write_user(&core, &root_token, "pass", "test", "123qwe!@#", 0).await;
        let resp = test_read_user(&core, &root_token, "test").await.unwrap();
        assert!(resp.is_some());

        test_delete_user(&core, &root_token, "test").await;
        let resp = test_read_user(&core, &root_token, "test").await.unwrap();
        assert!(resp.is_none());

        test_write_user(&core, &root_token, "pass", "test", "123qwe!@#", 0).await;
        let _ = test_login(&core, "pass", "test", "123qwe!@#", true).await;
        let _ = test_login(&core, "pass", "test", "xxxxxxx", false).await;
        let _ = test_login(&core, "pass", "xxxx", "123qwe!@#", false).await;
        let resp = test_login(&core, "pass", "test", "123qwe!@#", true).await;
        let login_auth = resp.unwrap().unwrap().auth.unwrap();
        let test_client_token = login_auth.client_token.clone();
        let resp = test_read_api(&core, &test_client_token, "auth/token/lookup-self", true).await;
        println!("read auth/token/lookup-self resp: {:?}", resp);
        assert!(resp.unwrap().is_some());

        test_delete_user(&core, &root_token, "test").await;
        let resp = test_login(&core, "pass", "test", "123qwe!@#", false).await;
        let login_resp = resp.unwrap().unwrap();
        assert!(login_resp.auth.is_none());

        test_write_user(&core, &root_token, "pass", "test2", "123qwe", 5).await;
        let resp = test_read_user(&core, &root_token, "test").await.unwrap();
        assert!(resp.is_none());
        let resp = test_login(&core, "pass", "test2", "123qwe", true).await;
        let login_auth = resp.unwrap().unwrap().auth.unwrap();
        println!("user login_auth: {:?}", login_auth);
        assert_eq!(login_auth.lease.ttl.as_secs(), 5);

        println!("wait 7s");
        std::thread::sleep(Duration::from_secs(7));
        let test_client_token = login_auth.client_token.clone();
        let resp = test_read_api(&core, &test_client_token, "auth/token/lookup-self", false).await;
        println!("read auth/token/lookup-self resp: {:?}", resp);
        assert_eq!(resp.unwrap_err(), RvError::ErrPermissionDenied);

        // mount userpass auth to path: auth/testpass
        test_mount_auth_api(&core, &root_token, "userpass", "testpass").await;
        test_write_user(&core, &root_token, "testpass", "testuser", "123qwe!@#", 0).await;
        let resp = test_login(&core, "testpass", "testuser", "123qwe!@#", true).await;
        let login_auth = resp.unwrap().unwrap().auth.unwrap();
        let test_client_token = login_auth.client_token.clone();
        println!("test_client_token: {}", test_client_token);
        let resp = test_read_api(&core, &test_client_token, "auth/token/lookup-self", true).await;
        println!("read auth/token/lookup-self resp: {:?}", resp);
        assert!(resp.unwrap().is_some());
    }
}
