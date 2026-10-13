#!/usr/bin/env bash
# AUGECOIN Technical Book — Build Script
# Recreates the complete Technical Book from source.
#
# Usage:
#   ./build.sh              # Full rebuild
#   ./build.sh --skip-tests # Skip cargo test/clippy
#   ./build.sh --graphify   # Run graphify first

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BOOK_DIR="$SCRIPT_DIR"
OUTPUT_DIR="$BOOK_DIR/output"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[BOOK]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()  { echo -e "${RED}[ERROR]${NC} $*" >&2; }

SKIP_TESTS=false
RUN_GRAPHIFY=false

for arg in "$@"; do
  case "$arg" in
    --skip-tests) SKIP_TESTS=true ;;
    --graphify)   RUN_GRAPHIFY=true ;;
    --help|-h)
      echo "Usage: $0 [--skip-tests] [--graphify]"
      exit 0
      ;;
  esac
done

# ─── Step 1: Graphify (optional) ──────────────────────────
if [ "$RUN_GRAPHIFY" = true ]; then
  log "Running Graphify..."
  cd "$PROJECT_ROOT"
  if command -v graphify &>/dev/null; then
    graphify . --update --code-only || warn "Graphify failed, continuing..."
  else
    warn "graphify not found, skipping"
  fi
fi

# ─── Step 2: Cargo tests (optional) ───────────────────────
if [ "$SKIP_TESTS" = false ]; then
  log "Running cargo test..."
  cd "$PROJECT_ROOT"
  cargo test --workspace 2>&1 | tail -20 || warn "Some tests failed"

  log "Running cargo clippy..."
  cargo clippy --workspace --all-targets 2>&1 | tail -10 || warn "Clippy warnings found"
fi

# ─── Step 3: Generate book ────────────────────────────────
log "Generating Technical Book..."
mkdir -p "$OUTPUT_DIR"
python3 "$BOOK_DIR/build.py"

# ─── Step 4: Generate PDF (if pandoc available) ───────────
if command -v pandoc &>/dev/null; then
  log "Generating PDF with pandoc (xelatex)..."
  pandoc "$OUTPUT_DIR/AUGECOIN_Technical_Book.md" \
    -o "$OUTPUT_DIR/AUGECOIN_Technical_Book.pdf" \
    --pdf-engine=xelatex \
    -V mainfont="DejaVu Sans" \
    -V monofont="DejaVu Sans Mono" \
    --toc \
    --toc-depth=3 \
    -V geometry:margin=1in \
    -V fontsize=11pt \
    -V colorlinks=true \
    -V linkcolor=blue \
    2>&1 && log "PDF generated" || warn "PDF generation failed"
else
  warn "pandoc not found, skipping PDF generation"
fi

# ─── Step 5: Generate EPUB (if pandoc available) ──────────
if command -v pandoc &>/dev/null; then
  log "Generating EPUB..."
  pandoc "$OUTPUT_DIR/AUGECOIN_Technical_Book.md" \
    -o "$OUTPUT_DIR/AUGECOIN_Technical_Book.epub" \
    --toc \
    --toc-depth=3 \
    --metadata title="AUGECOIN Technical Book" \
    2>&1 && log "EPUB generated" || warn "EPUB generation failed"
fi

# ─── Summary ──────────────────────────────────────────────
log "Build complete!"
echo ""
echo "Output files:"
ls -lh "$OUTPUT_DIR"/*.{md,html,pdf,epub,json} 2>/dev/null || true
echo ""
echo "SHA256 hashes:"
if [ -f "$OUTPUT_DIR/sha256sums.json" ]; then
  cat "$OUTPUT_DIR/sha256sums.json"
fi
