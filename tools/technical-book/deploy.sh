#!/usr/bin/env bash
# Deploy AUGECOIN Technical Book to augecoin.in/fonte
# Usage: ./deploy.sh

set -euo pipefail

WEB_DIR="$(cd "$(dirname "$0")/web" && pwd)"
OUTPUT_DIR="$(cd "$(dirname "$0")/output" && pwd)"

echo "========================================="
echo "  AUGECOIN Technical Book — Deploy"
echo "========================================="

# Copy web assets
echo "[1/3] Preparing web directory..."
mkdir -p "$OUTPUT_DIR/web"
cp "$WEB_DIR/index.html" "$OUTPUT_DIR/web/"

# Copy generated files alongside
cp "$OUTPUT_DIR/AUGECOIN_Technical_Book.md" "$OUTPUT_DIR/web/"
cp "$OUTPUT_DIR/AUGECOIN_Technical_Book.pdf" "$OUTPUT_DIR/web/" 2>/dev/null || true
cp "$OUTPUT_DIR/AUGECOIN_Technical_Book.epub" "$OUTPUT_DIR/web/" 2>/dev/null || true

echo "[2/3] Files ready:"
ls -lh "$OUTPUT_DIR/web/"

echo ""
echo "[3/3] Deploy options:"
echo ""
echo "  Option 1 — Nginx/Caddy:"
echo "    Copy $OUTPUT_DIR/web/* to your web root"
echo "    Example: cp -r $OUTPUT_DIR/web/* /var/www/augecoin.in/fonte/"
echo ""
echo "  Option 2 — GitHub Pages:"
echo "    Push the web/ folder to a gh-pages branch"
echo ""
echo "  Option 3 — Cloudflare Pages / Vercel / Netlify:"
echo "    Point to: $OUTPUT_DIR/web/"
echo ""
echo "  Option 4 — Simple HTTP server (test):"
echo "    cd $OUTPUT_DIR/web && python3 -m http.server 8080"
echo ""
echo "========================================="
