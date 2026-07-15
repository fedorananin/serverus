use std::net::TcpListener;
use std::time::Duration;

use anyhow::{bail, Context, Result};

pub(crate) fn reserve_local_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("reserve local fixture port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("read reserved fixture port")
}

pub(crate) async fn wait_for_listener(port: u16) -> Result<()> {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    bail!("fixture did not listen on its reserved port")
}
