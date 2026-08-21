#!/usr/bin/env bash
#
# Build Excavation for the browser (wasm32-unknown-unknown) and stage the web
# artifacts in ./web.
#
# The loader (mq_js_bundle.js) and the assets are vendored/copied locally so the
# page loads with no CDN. Serve the result with a static server, e.g.:
#
#   python3 -m http.server -d web
#
# Usage: ./build-web.sh [profile]
#   profile: release (default) or debug

set -euo pipefail

cd "$(dirname "$0")"
PROFILE="${1:-release}"

if [ "$PROFILE" = "release" ]; then
    cargo build --release --target wasm32-unknown-unknown
else
    cargo build --target wasm32-unknown-unknown
fi

WASM="target/wasm32-unknown-unknown/${PROFILE}/excavation.wasm"
if [ ! -f "$WASM" ]; then
    echo "error: expected wasm at $WASM" >&2
    exit 1
fi

mkdir -p web
cp "$WASM" web/excavation.wasm

# Vendor the macroquad JS loader locally (no CDN).
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
MQ_JS=$(find "$CARGO_HOME_DIR" -path '*macroquad-0.4.*/js/mq_js_bundle.js' 2>/dev/null | head -1)
if [ -z "$MQ_JS" ]; then
    echo "error: could not locate macroquad js/mq_js_bundle.js" >&2
    exit 1
fi
cp "$MQ_JS" web/mq_js_bundle.js

# Copy assets so the wasm can fetch them in the browser.
rm -rf web/assets
cp -r assets web/assets

echo "Web build staged in ./web"
echo "Serve with: python3 -m http.server -d web"
