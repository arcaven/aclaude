#!/usr/bin/env bash
# Upload packed portrait archives and manifest to Cloudflare R2.
#
# Prerequisites:
#   - rclone configured with an "r2" remote (see below)
#   - R2 bucket "forestage-portraits" created (wrangler r2 bucket create forestage-portraits)
#
# rclone setup (one-time):
#   rclone config create r2 s3 \
#     provider=Cloudflare \
#     access_key_id=<your-r2-access-key> \
#     secret_access_key=<your-r2-secret-key> \
#     endpoint=https://<account-id>.r2.cloudflarestorage.com \
#     acl=private
#
# Usage:
#   ./scripts/portraits/upload-r2.sh <dist-dir>
#   ./scripts/portraits/upload-r2.sh dist/portraits

set -euo pipefail

DIST_DIR="${1:?Usage: upload-r2.sh <dist-dir>}"

if ! command -v rclone &>/dev/null; then
    echo "Error: rclone not found. Install with: brew install rclone"
    exit 1
fi

if [[ ! -f "$DIST_DIR/manifest.json" ]]; then
    echo "Error: manifest.json not found in $DIST_DIR. Run gen-manifest.sh first."
    exit 1
fi

pack_count=$(ls "$DIST_DIR"/*.tar.gz 2>/dev/null | wc -l | tr -d ' ')
echo "Uploading $pack_count theme packs + manifest to R2..."

echo ""
echo "--- Theme packs (immutable, 7d cache) ---"
# --s3-no-check-bucket: R2 Object-only tokens can't call HeadBucket;
# without this flag, rclone falls back to CreateBucket (also denied).
# sync works without it (ListObjects confirms bucket), but adding it
# here for consistency.
rclone sync "$DIST_DIR/" r2:forestage-portraits/v1/themes/ \
    --include "*.tar.gz" --include "*.sha256" \
    --s3-no-check-bucket \
    --progress

echo ""
echo "--- Manifest (1h cache, etag-gated) ---"
rclone copyto "$DIST_DIR/manifest.json" r2:forestage-portraits/v1/manifest.json \
    --s3-no-check-bucket \
    --progress

echo ""
echo "Done. Verify:"
echo "  curl -sI https://portraits.darkatelier.org/v1/manifest.json | head -10"
echo "  curl -s https://portraits.darkatelier.org/v1/manifest.json | head -5"
