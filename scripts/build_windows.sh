#!/usr/bin/env bash
# Builds dist/Shizuka-windows.zip from macOS using cargo-xwin.
# Requires: cargo install cargo-xwin
set -euo pipefail
cd "$(dirname "$0")/.."

# Make sure the rustup toolchain (which has the Windows target) and cargo-xwin
# take precedence over Homebrew's rust.
export PATH="$(dirname "$(rustup which cargo)"):$HOME/.cargo/bin:$PATH"

DIST="dist"
mkdir -p "$DIST"

# Static CRT so the exe does not require the Visual C++ redistributable.
RUSTFLAGS="-C target-feature=+crt-static" \
    cargo xwin build --release --target x86_64-pc-windows-msvc

cp "target/x86_64-pc-windows-msvc/release/mobile_app.exe" "$DIST/Shizuka.exe"

rm -f "$DIST/Shizuka-windows.zip"
(
    cd "$DIST"
    zip -9 -q Shizuka-windows.zip Shizuka.exe
)
rm -f "$DIST/Shizuka.exe"

echo "Built $DIST/Shizuka-windows.zip"
