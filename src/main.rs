//! Nyancat, served three ways.
//!
//! A Rust rewrite of [nyancat](https://github.com/klange/nyancat) and
//! [nyancatsh](https://github.com/aymanbagabas/nyancatsh): the same animation
//! streamed over SSH and Telnet, or played locally with `--raw`.
//!
//! - `art` — the frame data and color map.
//! - `render` — the shared, stateless [`render::Animation`].
//! - `ssh` / `telnet` — the two network transports.
//! - `local` — `--raw` terminal playback.
//! - `cli` — argument parsing.

mod art;
mod cli;
mod local;
mod render;
mod ssh;
mod telnet;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;

use cli::Args;
use render::Animation;

fn main() -> Result<()> {
    let args = Args::parse();
    let animation = Arc::new(Animation::new());

    if args.raw {
        // Raw playback owns the terminal, so we run no servers and emit no logs.
        return local::run(animation);
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Telnet is blocking std::net, so it gets its own OS thread.
    let telnet_animation = animation.clone();
    let telnet_addr = args.telnet.0;
    let telnet_handle = std::thread::spawn(move || {
        if let Err(e) = telnet::run(telnet_animation, telnet_addr) {
            log::error!("telnet server error: {e}");
        }
    });

    // SSH is async; drive it on a Tokio runtime (runs until the process exits).
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        if let Err(e) = ssh::run(animation, args.ssh.0, args.host_key).await {
            log::error!("ssh server error: {e}");
        }
    });

    let _ = telnet_handle.join();
    Ok(())
}
