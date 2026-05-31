//! End-to-end test: spawn the built binary and confirm the Telnet server greets
//! a client, streams animation frames (ANSI background escapes), waves goodbye
//! when asked to quit, and never leaks log/debug noise into the stream.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use common::TestServer;

/// Connect to `port` on localhost, retrying while the freshly spawned server
/// finishes binding.
fn connect(port: u16) -> TcpStream {
    for _ in 0..50 {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
            return stream;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("telnet server never accepted a connection");
}

#[test]
fn telnet_streams_frames() {
    let server = TestServer::spawn();
    let mut stream = connect(server.telnet_port);
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
}

#[test]
fn telnet_says_goodbye_on_quit() {
    let server = TestServer::spawn();
    let mut stream = connect(server.telnet_port);
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();

    // Drain the opening banner / first frames so our 'q' is read as fresh input.
    let mut buf = [0u8; 8192];
    let _ = stream.read(&mut buf);

    // Press 'q': the server should send a parting message and close the stream.
    stream.write_all(b"q").unwrap();
    stream.flush().unwrap();

    let mut collected = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break, // server closed the connection — expected after quit
            Ok(n) => collected.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
        if collected.len() > 200_000 {
            break;
        }
    }

    let text = String::from_utf8_lossy(&collected);
    assert!(
        text.contains("Thanks for stopping by"),
        "expected a goodbye message after pressing 'q'"
    );
}
