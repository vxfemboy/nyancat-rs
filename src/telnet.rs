//! Telnet transport.
//!
//! A thread-per-client TCP server that negotiates a few Telnet options (notably
//! NAWS for window size) and then streams animation frames. Like the SSH side it
//! never offers a shell — the only client input it acts on is "quit".

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use log::{error, info};

use crate::art::INTERVAL;
use crate::render::Animation;

/// IAC (Interpret As Command) — the Telnet escape byte.
const IAC: u8 = 255;
/// SB (Subnegotiation Begin).
const SB: u8 = 250;
/// NAWS (Negotiate About Window Size) option code.
const NAWS: u8 = 31;

/// Telnet option negotiation sent on connect: server will echo and suppress
/// go-ahead, and asks the client to report its window size (NAWS).
const NEGOTIATION: &[u8] = &[
    IAC, 251, 1, // WILL ECHO
    IAC, 251, 3, // WILL SUPPRESS-GO-AHEAD
    IAC, 253, 3, // DO SUPPRESS-GO-AHEAD
    IAC, 253, NAWS, // DO NAWS
];

/// Subnegotiation End.
const SE: u8 = 240;

/// Returns true if `byte` is a quit request: 'q' or Ctrl-C (0x03).
fn is_quit(byte: u8) -> bool {
    byte == b'q' || byte == 0x03
}

/// Outcome of scanning a chunk of bytes from a Telnet client.
#[derive(Debug, Default, PartialEq)]
struct TelnetInput {
    /// Window size, if a NAWS subnegotiation was present.
    naws: Option<(u16, u16)>,
    /// Whether the user pressed quit in the *data* stream. Bytes belonging to
    /// IAC commands and subnegotiations are excluded — option codes like
    /// SUPPRESS-GO-AHEAD (3) and window dimensions (which can be 'q' = 0x71 or
    /// 0x03) must not be mistaken for keystrokes.
    quit: bool,
}

/// Walk a Telnet byte stream, skipping IAC command/subnegotiation sequences and
/// inspecting only genuine data bytes. Extracts NAWS window size along the way.
fn process_telnet(buf: &[u8]) -> TelnetInput {
    let mut out = TelnetInput::default();
    let mut i = 0;
    while i < buf.len() {
        if buf[i] != IAC {
            if is_quit(buf[i]) {
                out.quit = true;
            }
            i += 1;
            continue;
        }
        // buf[i] == IAC; need a command byte.
        let Some(&cmd) = buf.get(i + 1) else { break };
        match cmd {
            // Escaped literal 0xFF — a data byte, never a quit key.
            IAC => i += 2,
            // WILL / WONT / DO / DONT are followed by a single option byte.
            251..=254 => i += 3,
            SB => {
                let opt = buf.get(i + 2).copied();
                // Find the terminating IAC SE.
                let mut j = i + 2;
                let end = loop {
                    if j + 1 >= buf.len() {
                        break buf.len();
                    }
                    if buf[j] == IAC && buf[j + 1] == SE {
                        break j;
                    }
                    j += 1;
                };
                if opt == Some(NAWS) {
                    let payload = &buf[(i + 3).min(end)..end];
                    if payload.len() >= 4 {
                        out.naws = Some((
                            u16::from_be_bytes([payload[0], payload[1]]),
                            u16::from_be_bytes([payload[2], payload[3]]),
                        ));
                    }
                }
                i = if end < buf.len() { end + 2 } else { buf.len() };
            }
            // Any other 2-byte command (SE, NOP, IP, …).
            _ => i += 2,
        }
    }
    out
}

fn handle_client(mut stream: TcpStream, animation: Arc<Animation>) -> io::Result<()> {
    stream.set_nonblocking(true)?;
    stream.write_all(NEGOTIATION)?;

    let welcome = "\r\n\x1B[1;36mWelcome to NyanCat!\x1B[0m\r\n\
                   \x1B[1;33mPress 'q' or Ctrl+C to exit\x1B[0m\r\n\r\n";
    stream.write_all(welcome.as_bytes())?;
    stream.flush()?;

    let mut width = 80usize;
    let mut height = 24usize;
    let mut frame_index = 0usize;
    let mut buffer = [0u8; 1024];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break, // client disconnected
            Ok(n) => {
                let input = process_telnet(&buffer[..n]);
                if let Some((w, h)) = input.naws {
                    if w > 0 && h > 0 {
                        width = w as usize;
                        height = h as usize;
                    }
                }
                if input.quit {
                    let _ =
                        stream.write_all(b"\r\n\x1B[1;36mThanks for stopping by! :3\x1B[0m\r\n");
                    let _ = stream.flush();
                    break;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }

        let frame = animation.render(frame_index, width, height);
        stream.write_all(frame.as_bytes())?;
        stream.flush()?;
        frame_index = frame_index.wrapping_add(1);

        thread::sleep(INTERVAL);
    }

    Ok(())
}

/// Run the Telnet server forever, binding `addr`.
pub fn run(animation: Arc<Animation>, addr: SocketAddr) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    info!("Telnet listening on {addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let animation = animation.clone();
                thread::spawn(move || {
                    // A dropped connection is normal; ignore the resulting error.
                    let _ = handle_client(stream, animation);
                });
            }
            Err(e) => error!("telnet accept error: {e}"),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_quit_bytes() {
        assert!(is_quit(b'q'));
        assert!(is_quit(0x03));
        assert!(!is_quit(b'a'));
        assert!(!is_quit(b'Q'));
    }

    #[test]
    fn real_keystrokes_quit() {
        assert!(process_telnet(b"q").quit);
        assert!(process_telnet(&[0x03]).quit);
        assert!(!process_telnet(b"hello").quit);
    }

    #[test]
    fn sga_negotiation_is_not_a_quit() {
        // The original bug: a client's reply to our SUPPRESS-GO-AHEAD (option 3)
        // contains the byte 0x03, which must NOT be read as Ctrl-C.
        assert!(!process_telnet(&[IAC, 253, 3]).quit); // IAC DO SGA
        assert!(!process_telnet(&[IAC, 251, 3]).quit); // IAC WILL SGA
        assert!(!process_telnet(&[IAC, 254, 3, IAC, 252, 3]).quit); // DONT/WONT SGA
    }

    #[test]
    fn naws_dimensions_are_not_quit_keys() {
        // width=113 ('q'=0x71), height=3 (0x03) — both appear in the payload but
        // are window dimensions, not keystrokes.
        let buf = [IAC, SB, NAWS, 0x00, 0x71, 0x00, 0x03, IAC, SE];
        let input = process_telnet(&buf);
        assert_eq!(input.naws, Some((113, 3)));
        assert!(!input.quit);
    }

    #[test]
    fn parses_naws_window_size() {
        // IAC SB NAWS, width=0x0050 (80), height=0x0018 (24)
        let buf = [IAC, SB, NAWS, 0x00, 0x50, 0x00, 0x18, IAC, SE];
        assert_eq!(process_telnet(&buf).naws, Some((80, 24)));
    }

    #[test]
    fn negotiation_then_real_quit() {
        // Option bytes are skipped, but a trailing 'q' keystroke still quits.
        let buf = [IAC, 253, 3, IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE, b'q'];
        let input = process_telnet(&buf);
        assert_eq!(input.naws, Some((80, 24)));
        assert!(input.quit);
    }

    #[test]
    fn plain_data_without_iac() {
        assert_eq!(process_telnet(b"abc"), TelnetInput::default());
    }
}
