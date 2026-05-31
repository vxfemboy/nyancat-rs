//! End-to-end test: spawn the built binary and confirm the Telnet server
//! greets a client and streams animation frames (ANSI background escapes) with
//! no log/debug noise mixed into the stream.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command};
use std::time::Duration;

/// Grab a currently-free localhost port by binding to :0 and releasing it.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn telnet_streams_frames() {
    let telnet_port = free_port();
    let ssh_port = free_port();
    let host_key = std::env::temp_dir().join(format!("nyancat_test_key_{}", std::process::id()));
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
    let _server = Server(child);

    // Give the server a moment to bind.
    let mut stream = None;
    for _ in 0..50 {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", telnet_port)) {
            stream = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut stream = stream.expect("server never accepted a connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();

    // Read enough bytes to capture the banner plus at least one frame. We never
    // write, so the server doesn't see EOF and keeps streaming.
    let mut collected = Vec::new();
    let mut buf = [0u8; 8192];
    while collected.len() < 5000 {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => collected.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }

    let text = String::from_utf8_lossy(&collected);
    assert!(
        text.contains("Welcome to NyanCat"),
        "missing welcome banner"
    );
    assert!(
        collected.windows(7).any(|w| w == b"\x1b[48;5;"),
        "no ANSI background escapes — server did not stream frames"
    );
    assert!(
        !text.contains("INFO") && !text.contains("nyancat::"),
        "log output leaked into the client stream"
    );

    let _ = std::fs::remove_file(&host_key);
}
