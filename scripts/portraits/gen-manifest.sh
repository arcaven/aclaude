#!/usr/bin/env bash
# Generate manifest.json from packed theme archives.
#
# The manifest has one section:
#   themes: pack metadata (sha256, size, persona count)
#
# Pre-B14 versions of this script also emitted a `personas` map (role
# → filename-stem) consumed by an old portrait.rs lookup path. That
# map became inert once portrait resolution moved to deriving stems
# from the Character itself (orc finding-033, B14 agent taxonomy).
# Removed entirely from this script + src/download.rs in the same
# pass; serde ignores any leftover field on already-published
# manifests.
#
# Usage:
#   ./scripts/portraits/gen-manifest.sh <dist-dir> [base-url]
#   ./scripts/portraits/gen-manifest.sh dist/portraits

set -euo pipefail

DIST_DIR="${1:?Usage: gen-manifest.sh <dist-dir> [base-url]}"
BASE_URL="${2:-https://portraits.darkatelier.org/v1}"

if [[ ! -d "$DIST_DIR" ]]; then
    echo "Error: dist directory not found: $DIST_DIR" >&2
    exit 1
fi

# Collect theme slugs from packed archives
themes=()
for pack in "$DIST_DIR"/*.tar.gz; do
    [[ ! -f "$pack" ]] && continue
    themes+=("$(basename "$pack" .tar.gz)")
done

if [[ ${#themes[@]} -eq 0 ]]; then
    echo "Error: no .tar.gz files found in $DIST_DIR" >&2
    exit 1
fi

# --- Build JSON ---

# Header
printf '{\n'
printf '  "schema": 1,\n'
printf '  "updated": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '  "base_url": "%s",\n' "$BASE_URL"

# Themes section
printf '  "themes": {\n'
first=true
for theme in "${themes[@]}"; do
    pack="$DIST_DIR/${theme}.tar.gz"
    sha=$(cat "$DIST_DIR/${theme}.sha256")
    # stat -f%z (macOS) or stat -c%s (Linux)
    bytes=$(stat -f%z "$pack" 2>/dev/null || stat -c%s "$pack" 2>/dev/null)
    persona_count=$(tar tzf "$pack" | grep "original/.*\.png$" | wc -l | tr -d ' ')

    $first || printf ',\n'
    first=false
    printf '    "%s": {"pack_sha256": "%s", "pack_bytes": %s, "persona_count": %s}' \
        "$theme" "$sha" "$bytes" "$persona_count"
done
printf '\n  }\n'
printf '}\n'
