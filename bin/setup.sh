#!/usr/bin/env bash
# Set up and validate the development container. Safe to run repeatedly.
# Runs automatically after the Dev Container is created (postCreateCommand),
# and can be run by hand at any time: bin/setup.sh
set -euo pipefail

cd "$(dirname "$0")/.."

failures=0
step() { printf '\n==> %s\n' "$*"; }
ok()   { printf '    ok: %s\n' "$*"; }
fail() { printf '    FAIL: %s\n' "$*"; failures=$((failures + 1)); }

step "Checking toolchain"
for tool in rustc cargo rustup; do
  if command -v "$tool" >/dev/null; then ok "$("$tool" --version | head -n1)"; else fail "$tool not found"; fi
done

step "Installing Rust components"
rustup component add rustfmt clippy >/dev/null 2>&1 && ok "rustfmt and clippy installed" || fail "rustup component add"

step "Fetching dependencies"
cargo fetch --locked && ok "dependencies match Cargo.lock" || fail "cargo fetch --locked"

step "Checking formatting"
cargo fmt --check && ok "formatted" || fail "run 'cargo fmt'"

step "Running clippy"
cargo clippy --all-targets --locked -- -D warnings && ok "no warnings" || fail "clippy"

step "Running tests"
cargo test --locked && ok "tests pass" || fail "tests"

echo
if [ "$failures" -eq 0 ]; then
  echo "Setup complete: all checks passed."
else
  echo "Setup finished with $failures failure(s); see FAIL lines above."
  exit 1
fi
