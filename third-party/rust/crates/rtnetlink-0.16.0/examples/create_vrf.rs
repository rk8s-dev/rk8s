// SPDX-License-Identifier: MIT

use rtnetlink::{new_connection, LinkVrf};

#[tokio::main]
async fn main() -> Result<(), String> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    handle
        .link()
        .add(LinkVrf::new("my-vrf", 101).build())
        .execute()
        .await
        .map_err(|e| format!("{e}"))
}
