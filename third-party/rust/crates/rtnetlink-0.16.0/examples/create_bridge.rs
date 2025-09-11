// SPDX-License-Identifier: MIT

use rtnetlink::{new_connection, LinkBridge};

#[tokio::main]
async fn main() -> Result<(), String> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    handle
        .link()
        .add(LinkBridge::new("my-bridge").build())
        .execute()
        .await
        .map_err(|e| format!("{e}"))
}
