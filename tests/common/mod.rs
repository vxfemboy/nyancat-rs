//! Shared helpers for the network integration tests: grab free localhost ports
//! and launch the `nyancat` binary with both servers bound, cleaning everything
//! up on drop.
//!
//! Each integration-test crate that needs a live server pulls this in with
//! `mod common;`. Not every helper is used by every test crate, hence the
//! crate-level `dead_code` allow.
#![allow(dead_code)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command};

/// Grab a currently-free localhost port by binding to `:0` and releasing it.
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A spawned `nyancat` process plus the ports and throwaway host-key path it was
/// launched with. Killed and cleaned up when dropped.
pub struct TestServer {
    child: Child,
    pub telnet_port: u16,
    pub ssh_port: u16,
    pub host_key: PathBuf,
}

impl TestServer {
    /// Launch the binary with both servers on fresh localhost ports and a
    /// disposable host key.
    pub fn spawn() -> Self {
        let telnet_port = free_port();
        let ssh_port = free_port();
        let host_key = std::env::temp_dir().join(format!(
            "nyancat_test_key_{}_{}",
            std::process::id(),
            ssh_port
        ));
        let _ = std::fs::remove_file(&host_key);

        let child = Command::new(env!("CARGO_BIN_EXE_nyancat"))
            .args([
                "--telnet",
                &format!("127.0.0.1:{telnet_port}"),
                "--ssh",
                &format!("127.0.0.1:{ssh_port}"),
                "--host-key",
                host_key.to_str().unwrap(),
            ])
            .spawn()
            .expect("failed to launch nyancat binary");

        Self {
            child,
            telnet_port,
            ssh_port,
            host_key,
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.host_key);
    }
}
