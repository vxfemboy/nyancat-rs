//! Secure SSH transport, built on [`russh`].
//!
//! Security model: authentication is open (anyone can connect, like the upstream
//! `nyancatsh`), but a connection is granted **nothing** except the animation
//! stream. We accept a session channel, a pty, and a shell request — then stream
//! frames. Every other channel request (`exec`, `subsystem`, …) is explicitly
//! refused, and no process is ever spawned, so there is no shell or command to
//! escape into. All traffic is encrypted by russh; the broken hand-rolled crypto
//! that used to live here is gone.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use log::{debug, info};
use russh::keys::ssh_key::{self, private::Ed25519Keypair, LineEnding, PrivateKey};
use russh::server::{Auth, Config, Handle, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, Pty};
use tokio::task::JoinHandle;

use crate::art::INTERVAL;
use crate::render::Animation;

/// Default terminal size used until the client negotiates a pty / window size.
const DEFAULT_WIDTH: usize = 80;
const DEFAULT_HEIGHT: usize = 24;

/// Run the SSH server forever, binding `addr` and using (or creating) the host
/// key at `host_key_path`.
pub async fn run(
    animation: Arc<Animation>,
    addr: SocketAddr,
    host_key_path: PathBuf,
) -> Result<()> {
    let host_key = load_or_create_host_key(&host_key_path)?;

    let config = Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
        auth_rejection_time: std::time::Duration::from_secs(3),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![host_key],
        nodelay: true,
        ..Default::default()
    };

    let mut server = NyanServer { animation };
    info!("SSH listening on {addr}");
    server.run_on_address(Arc::new(config), addr).await?;
    Ok(())
}

/// Read the ed25519 host key from `path`, generating and persisting a new one if
/// the file does not exist. Persisting it keeps the server's identity stable so
/// clients don't see host-key-changed warnings across restarts.
fn load_or_create_host_key(path: &Path) -> Result<PrivateKey> {
    if path.exists() {
        let key = PrivateKey::read_openssh_file(path)
            .with_context(|| format!("reading SSH host key {}", path.display()))?;
        debug!("loaded SSH host key from {}", path.display());
        Ok(key)
    } else {
        // Generate the seed with `rand` (whose `rand_core` differs from ssh-key's),
        // then build the key from the seed to avoid a cross-version RNG trait clash.
        let seed: [u8; 32] = rand::random();
        let key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
        key.write_openssh_file(path, LineEnding::LF)
            .with_context(|| format!("writing SSH host key {}", path.display()))?;
        info!("generated new SSH host key at {}", path.display());
        Ok(key)
    }
}

/// Connection factory. Cloned per incoming connection by russh.
#[derive(Clone)]
struct NyanServer {
    animation: Arc<Animation>,
}

impl Server for NyanServer {
    type Handler = NyanHandler;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> NyanHandler {
        NyanHandler::new(self.animation.clone())
    }
}

/// Per-connection state.
struct NyanHandler {
    animation: Arc<Animation>,
    /// Shared with the streaming task; updated on pty/window-change requests.
    dims: Arc<Mutex<(usize, usize)>>,
    /// The background frame-pump task, if a shell has been started.
    anim_task: Option<JoinHandle<()>>,
}

impl NyanHandler {
    fn new(animation: Arc<Animation>) -> Self {
        Self {
            animation,
            dims: Arc::new(Mutex::new((DEFAULT_WIDTH, DEFAULT_HEIGHT))),
            anim_task: None,
        }
    }

    fn set_dims(&self, width: u32, height: u32) {
        if let Ok(mut d) = self.dims.lock() {
            *d = (width.max(1) as usize, height.max(1) as usize);
        }
    }

    /// Spawn the task that renders frames and pushes them to the client channel.
    fn start_stream(&mut self, handle: Handle, channel: ChannelId) {
        if self.anim_task.is_some() {
            return;
        }
        let animation = self.animation.clone();
        let dims = self.dims.clone();
        let task = tokio::spawn(async move {
            let mut idx = 0usize;
            loop {
                let (w, h) = dims
                    .lock()
                    .map(|d| *d)
                    .unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));
                let frame = animation.render(idx, w, h);
                if handle.data(channel, frame.into_bytes()).await.is_err() {
                    break; // client went away
                }
                idx = idx.wrapping_add(1);
                tokio::time::sleep(INTERVAL).await;
            }
        });
        self.anim_task = Some(task);
    }

    fn stop_stream(&mut self) {
        if let Some(task) = self.anim_task.take() {
            task.abort();
        }
    }
}

impl Drop for NyanHandler {
    fn drop(&mut self) {
        self.stop_stream();
    }
}

impl Handler for NyanHandler {
    type Error = anyhow::Error;

    // --- Authentication: open to everyone, but grants nothing but the stream. ---

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.set_dims(col_width, row_height);
        session.channel_success(channel)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.set_dims(col_width, row_height);
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.start_stream(session.handle(), channel);
        session.channel_success(channel)?;
        Ok(())
    }

    /// Refuse command execution — this is what makes the server un-escapable.
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }

    /// Refuse subsystems (e.g. sftp).
    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        _name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // The only input we honor is "quit": 'q' or Ctrl-C (0x03).
        if data.contains(&b'q') || data.contains(&0x03) {
            self.stop_stream();
            session.close(channel)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_key_is_generated_then_reused() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("nyancat_keytest_{}.key", std::process::id()));
        let _ = std::fs::remove_file(&path);

        // First call generates and persists a fresh key.
        assert!(!path.exists());
        let first = load_or_create_host_key(&path).expect("generate key");
        assert!(path.exists(), "host key should be persisted to disk");

        // Second call must load the *same* key, not generate a new one.
        let second = load_or_create_host_key(&path).expect("reload key");
        assert_eq!(
            first.fingerprint(ssh_key::HashAlg::Sha256),
            second.fingerprint(ssh_key::HashAlg::Sha256),
            "reloading the host key should yield a stable identity"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dims_never_collapse_to_zero() {
        let handler = NyanHandler::new(Arc::new(Animation::new()));
        // A client reporting a 0-wide/0-tall window must be clamped to >= 1 so
        // rendering never divides by or indexes past zero.
        handler.set_dims(0, 0);
        let (w, h) = *handler.dims.lock().unwrap();
        assert_eq!((w, h), (1, 1));

        handler.set_dims(120, 40);
        let (w, h) = *handler.dims.lock().unwrap();
        assert_eq!((w, h), (120, 40));
    }
}
