#!/usr/bin/env bash
# Build Sorcerer and install the `sorce` binary.
#
#   ./install.sh                     # -> ~/.local/bin/sorce
#   PREFIX=/usr/local sudo ./install.sh
#   PREFIX="$HOME/.cargo" ./install.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="${BIN_DIR:-$PREFIX/bin}"

if [[ -n "${DESTDIR:-}" ]]; then
    BIN_DIR="${DESTDIR}${BIN_DIR}"
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install Rust from https://rustup.rs" >&2
    exit 1
fi

echo "==> Building Sorcerer (release)"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

install -d "$BIN_DIR"
install -m 755 "$ROOT/target/release/sorce" "$BIN_DIR/sorce"

echo "==> Installed $BIN_DIR/sorce"
"$BIN_DIR/sorce" --version

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        echo
        echo "Add this directory to your PATH:"
        echo "  export PATH=\"$BIN_DIR:\$PATH\""
        ;;
esac
