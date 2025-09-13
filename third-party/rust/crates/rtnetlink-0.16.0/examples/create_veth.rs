// SPDX-License-Identifier: MIT

use rtnetlink::{new_connection, LinkVeth};
#[tokio::main]
async fn main() -> Result<(), String> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    handle
        .link()
        .add(LinkVeth::new("veth1", "veth1-peer").build())
        .execute()
        .await
        .map_err(|e| format!("{e}"))
}
