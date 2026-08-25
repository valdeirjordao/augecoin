#!/bin/bash
# AUGECOIN Validator Desktop — Build Instructions
#
# Requisitos:
#   - Node.js >= 18
#   - Rust >= 1.77 (rustup)
#   - Tauri system deps:
#     Linux:   sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
#     Windows: Microsoft Visual Studio C++ Build Tools + WebView2
#
# O sidecar do node (augecoin-node) é embutido a partir de
# src-tauri/binaries/augecoin-node-<target-triple>. Ele é gerado
# automaticamente abaixo para o alvo corrente (host).
#
# OTA: defina AUGECOIN_RELEASE_PUBLIC_KEY_HEX (chave pública Ed25519 em hex,
# 64 chars) no ambiente para habilitar a instalação OTA com verificação de
# assinatura. Sem ela, o OTA permanece desabilitado (seguro por padrão).

set -e

echo "=== AUGECOIN Validator Desktop Build ==="

TRIPLE=$(rustc -vV | sed -n 's/host: //p')
EXE=""
BUNDLES="deb,appimage"
case "$TRIPLE" in
  *windows*) EXE=".exe"; BUNDLES="msi,nsis" ;;
  *apple*)   BUNDLES="app,dmg" ;;
esac

# 0. Ensure the node sidecar is present for the current target
SIDECAR="src-tauri/binaries/augecoin-node-${TRIPLE}${EXE}"
if [ ! -f "$SIDECAR" ]; then
  echo "[0/4] Building augecoin-node sidecar (${TRIPLE})..."
  (cd "$(dirname "$0")/../.." && cargo build -p augecoin-node --release)
  mkdir -p src-tauri/binaries
  cp "../../target/release/augecoin-node${EXE}" "$SIDECAR"
fi

echo "[1/4] Installing npm dependencies..."
npm install

echo "[2/4] Building frontend..."
npm run build

echo "[3/4] Building the desktop app (bundles: ${BUNDLES})..."
npm run tauri build -- --bundles "${BUNDLES}"

echo ""
echo "=== Build complete ==="
echo "Installer: src-tauri/target/release/bundle/"
echo "  Linux:   .deb / .AppImage"
echo "  Windows: .msi / .nsis (cross-build via CI/host Windows)"
