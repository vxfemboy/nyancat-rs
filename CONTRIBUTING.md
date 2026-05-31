# Contributing =^.^=

Thanks for helping the cat fly! Pounce on a bug, add a frame, or sharpen a test —
all paws welcome.

## Development

```sh
cargo run                                  # both servers (SSH 2222, Telnet 2323)
cargo run -- --raw                         # play locally, no servers
cargo test                                 # unit + integration tests
cargo clippy --all-targets -- -D warnings  # lint (CI-enforced, keep it clean)
cargo fmt --all                            # format
```

Enable the pre-commit gate (runs fmt/clippy/check/test) once per clone:

```sh
git config core.hooksPath .githooks
```

CI runs the same checks on every push and pull request.

## Tests

- **Unit tests** live next to the code in `#[cfg(test)]` modules:
  - `art` — the color map covers the art alphabet; frames are well-formed.
  - `render` — clear/home prefix, char→color codes, cropping, index wrap.
  - `cli` — endpoint parsing (bare port, IPv4, IPv6, garbage).
  - `telnet` — the `process_telnet` parser (quit keys, NAWS, IAC handling).
  - `ssh` — host-key generate-then-reuse round-trip and window-size clamping.
  - `local` — the `--raw` quit-key logic (`q`, `Esc`, `Ctrl+C`).
- **Integration tests** in `tests/` spawn the real binary and talk to it over the
  wire. Shared launch helpers live in `tests/common/mod.rs`.
  - `tests/telnet.rs` — greets a client, streams frames, and waves goodbye on `q`.
  - `tests/ssh.rs` — drives a real `russh` client: open auth + shell streams
    frames, and `exec` is refused (the connection is display-only).

Run just one set with `cargo test --test telnet` or `cargo test --test ssh`.

If you touch a transport or the renderer, please keep its tests green and add a
new one for the behavior you changed. New animation frames belong in
`src/art.rs` (one source of truth for the art and palette).

Now go on, make the kitty purr. meow~
