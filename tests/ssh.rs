//! End-to-end test: spawn the built binary and drive a real `russh` client
//! against the SSH server. Confirms that open auth is accepted, a shell request
//! starts the frame pump, and the client actually receives animation frames
//! (ANSI background escapes) — while `exec` requests are refused, proving the
//! connection is display-only and offers no command execution.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestServer;
use russh::client::{self, Handle};
use russh::keys::ssh_key;
use russh::ChannelMsg;

/// Client handler that trusts the server's host key (we generated it ourselves
/// for this test) and otherwise does nothing.
struct Client;

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Connect to the SSH server, retrying while the freshly spawned process binds.
async fn connect(port: u16) -> Handle<Client> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    });

    for _ in 0..50 {
        match client::connect(config.clone(), ("127.0.0.1", port), Client).await {
            Ok(handle) => return handle,
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    panic!("ssh server never accepted a connection");
}

#[tokio::test]
async fn ssh_accepts_open_auth_and_streams_frames() {
    let server = TestServer::spawn();
    let mut session = connect(server.ssh_port).await;

    // Auth is open: anyone may connect (no credentials required).
    let auth = session
        .authenticate_none("nyan")
        .await
        .expect("auth request failed");
    assert!(auth.success(), "server should accept open (none) auth");

    let channel = session
        .channel_open_session()
        .await
        .expect("failed to open session channel");
    channel
        .request_pty(true, "xterm", 80, 24, 0, 0, &[])
        .await
        .expect("pty request failed");
    channel
        .request_shell(true)
        .await
        .expect("shell request failed");

    // Collect data until we see an ANSI background escape (a rendered frame).
    let mut channel = channel;
    let mut collected = Vec::new();
    let saw_frame = loop {
        if collected.windows(7).any(|w| w == b"\x1b[48;5;") {
            break true;
        }
        match tokio::time::timeout(Duration::from_secs(5), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { ref data })) => collected.extend_from_slice(data),
            Ok(Some(_)) => {}
            Ok(None) => break false,
            Err(_) => break false,
        }
        if collected.len() > 200_000 {
            break collected.windows(7).any(|w| w == b"\x1b[48;5;");
        }
    };

    assert!(
        saw_frame,
        "SSH shell did not stream any animation frames (no ANSI background escapes)"
    );
}

#[tokio::test]
async fn ssh_refuses_exec_requests() {
    let server = TestServer::spawn();
    let mut session = connect(server.ssh_port).await;

    session
        .authenticate_none("nyan")
        .await
        .expect("auth request failed");

    let channel = session
        .channel_open_session()
        .await
        .expect("failed to open session channel");

    // `want_reply = true` makes the server answer with success/failure. The
    // server must refuse exec — there is no command execution to escape into.
    channel
        .exec(true, "id")
        .await
        .expect("sending exec request failed");

    let mut channel = channel;
    let mut refused = false;
    let mut got_data = false;
    while let Ok(Some(msg)) = tokio::time::timeout(Duration::from_secs(5), channel.wait()).await {
        match msg {
            ChannelMsg::Failure => {
                refused = true;
                break;
            }
            ChannelMsg::Success => break, // would mean exec was accepted (bad)
            ChannelMsg::Data { .. } | ChannelMsg::ExtendedData { .. } => {
                got_data = true;
                break;
            }
            _ => {}
        }
    }

    assert!(
        refused && !got_data,
        "exec must be refused with a channel failure and produce no output"
    );
}
