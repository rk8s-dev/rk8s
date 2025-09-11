// SPDX-License-Identifier: MIT

use rtnetlink::{new_connection, LinkWireguard};

#[tokio::main]
async fn main() -> Result<(), String> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    handle
        .link()
        .add(LinkWireguard::new("my-wg").build())
        .execute()
        .await
        .map_err(|e| format!("{e}"))
}
