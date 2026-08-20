#!/bin/bash
# AUGECOIN Wallet Desktop — Build Instructions
#
# Requisitos:
#   - Node.js >= 18
#   - Rust >= 1.77 (rustup)
#   - Tauri system deps:
#     Linux:   sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
#     macOS:   Xcode Command Line Tools
#     Windows: Microsoft Visual Studio C++ Build Tools + WebView2

set -e

echo "=== AUGECOIN Wallet Desktop Build ==="

TRIPLE=$(rustc -vV | sed -n 's/host: //p')
BUNDLES="deb,appimage"
case "$TRIPLE" in
  *windows*) BUNDLES="msi,nsis" ;;
  *apple*)   BUNDLES="app,dmg" ;;
esac

echo "[1/3] Installing npm dependencies..."
npm install

echo "[2/3] Building frontend..."
npm run build

echo "[3/3] Building the desktop app (bundles: ${BUNDLES})..."
npm run tauri build -- --bundles "${BUNDLES}"

echo ""
echo "=== Build complete ==="
echo "Binary: src-tauri/target/release/augecoin-wallet-desktop"
echo "Installer: src-tauri/target/release/bundle/"
