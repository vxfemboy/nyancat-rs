# nyancat

Animated nyancat served over **SSH** and **Telnet**, or played locally in your terminal.



The SSH transport is built on [`russh`](https://github.com/Eugeny/russh), so all traffic is properly encrypted. Connections are **display-only**: anyone may connect, but the server never spawns a shell and refuses `exec`/`subsystem` requests, so there is no command execution or terminal to escape into.

## Usage

```sh
cargo run                 # SSH on 0.0.0.0:2222, Telnet on 0.0.0.0:2323
cargo run -- --raw        # play locally in this terminal (no servers)
```

Then connect:

```sh
ssh localhost -p 2222     # press q or Ctrl+C to quit
telnet localhost 2323
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--ssh [HOST:]PORT` | `0.0.0.0:2222` | SSH listen address. Bare port or `host:port`. |
| `--telnet [HOST:]PORT` | `0.0.0.0:2323` | Telnet listen address. Bare port or `host:port`. |
| `--raw` | off | Play the animation in this terminal; do **not** start the servers. |
| `--host-key PATH` | `nyancat_host_key` | SSH host key file. Created (ed25519) on first run and reused. |

```sh
cargo run -- --ssh 9000 --telnet 127.0.0.1:4000 --host-key /etc/nyancat/host_key
```

## Layout

- `art` — frame data and the character → ANSI color map.
- `render` — the shared, stateless `Animation` renderer.
- `ssh` / `telnet` — the two network transports.
- `local` — `--raw` terminal playback.
- `cli` — argument parsing.

# Credits
This is a Rust rewrite that merges
[klange/nyancat](https://github.com/klange/nyancat) *(the terminal classic)* and [aymanbagabas/nyancatsh](https://github.com/aymanbagabas/nyancatsh) *(nyancat over SSH)* into a single binary.