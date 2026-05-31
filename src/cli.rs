//! Command-line interface.

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::str::FromStr;

use clap::Parser;

/// A listen address parsed from either a bare port (`2222` → `0.0.0.0:2222`) or a
/// full `HOST:PORT` (including IPv6 `[::1]:22`).
#[derive(Clone, Debug)]
pub struct Endpoint(pub SocketAddr);

impl FromStr for Endpoint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // A bare port number binds all interfaces.
        if let Ok(port) = s.parse::<u16>() {
            return Ok(Endpoint(SocketAddr::from(([0, 0, 0, 0], port))));
        }
        // Otherwise expect HOST:PORT.
        s.to_socket_addrs()
            .map_err(|e| format!("invalid address '{s}': {e}"))?
            .next()
            .map(Endpoint)
            .ok_or_else(|| format!("could not resolve '{s}'"))
    }
}

/// Animated Nyancat served over SSH and Telnet — a Rust rewrite of nyancat + nyancatsh.
#[derive(Parser, Debug)]
#[command(name = "nyancat", version, about)]
pub struct Args {
    /// SSH listen address: a bare port or HOST:PORT.
    #[arg(long, value_name = "[HOST:]PORT", default_value = "2222")]
    pub ssh: Endpoint,

    /// Telnet listen address: a bare port or HOST:PORT.
    #[arg(long, value_name = "[HOST:]PORT", default_value = "2323")]
    pub telnet: Endpoint,

    /// Play the animation locally in this terminal instead of starting the servers.
    #[arg(long)]
    pub raw: bool,

    /// Path to the persisted SSH host key (created on first run).
    #[arg(long, value_name = "PATH", default_value = "nyancat_host_key")]
    pub host_key: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_port_binds_all_interfaces() {
        let ep: Endpoint = "2222".parse().unwrap();
        assert_eq!(ep.0, "0.0.0.0:2222".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn ipv4_host_port() {
        let ep: Endpoint = "127.0.0.1:9000".parse().unwrap();
        assert_eq!(ep.0, "127.0.0.1:9000".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn ipv6_host_port() {
        let ep: Endpoint = "[::1]:22".parse().unwrap();
        assert_eq!(ep.0, "[::1]:22".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn rejects_garbage() {
        assert!("not-an-address".parse::<Endpoint>().is_err());
        assert!("99999".parse::<Endpoint>().is_err()); // out of u16 range
    }
}
