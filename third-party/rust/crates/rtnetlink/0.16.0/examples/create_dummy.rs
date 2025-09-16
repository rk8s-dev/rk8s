// SPDX-License-Identifier: MIT

use rtnetlink::{new_connection, LinkDummy};

#[tokio::main]
async fn main() -> Result<(), String> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    handle
        .link()
        .add(LinkDummy::new("dummy0").build())
        .execute()
        .await
        .map_err(|e| format!("{e}"))
}
