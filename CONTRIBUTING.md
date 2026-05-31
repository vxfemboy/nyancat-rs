## Development

```sh
cargo test                                 # unit + integration tests
cargo clippy --all-targets -- -D warnings  # lint (CI-enforced)
cargo fmt --all                            # format
```

Enable the pre-commit gate (runs fmt/clippy/check/test) once per clone:

```sh
git config core.hooksPath .githooks
```

CI runs the same checks on every push and pull request.
