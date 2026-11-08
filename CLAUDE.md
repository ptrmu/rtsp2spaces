# Working in this repository

- [DESIGN.md](DESIGN.md) is the source of truth for commands, configuration, file layout, and phases.
- Implement only the phase the user requests. Stop at the phase's review stop; do not advance automatically.
- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --locked` before reporting a phase complete.
- Never commit, log, or print credentials. Secrets files (`*_secrets.json`) stay untracked.
- When a command is implemented and verified, document it in the README's Usage section.
- The `python/` directory is reference material; do not modify it except to remove embedded credentials.
