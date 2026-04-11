#!/usr/bin/env bash
# Generate manifest.json from packed theme archives and theme YAML files.
#
# The manifest has two sections:
#   themes:   pack metadata (sha256, size, persona count)
#   personas: role → filename-stem mapping (exact format portrait.rs expects)
#
# Requires: yq (https://github.com/mikefarah/yq)
#
# Usage:
#   ./scripts/portraits/gen-manifest.sh <dist-dir> <themes-yaml-dir> [base-url]
#   ./scripts/portraits/gen-manifest.sh dist/portraits ~/work/penny-orc/pennyfarthing/pennyfarthing-dist/personas/themes

set -euo pipefail

DIST_DIR="${1:?Usage: gen-manifest.sh <dist-dir> <themes-yaml-dir> [base-url]}"
THEMES_DIR="${2:?Usage: gen-manifest.sh <dist-dir> <themes-yaml-dir> [base-url]}"
BASE_URL="${3:-https://portraits.darkatelier.org/v1}"

if ! command -v yq &>/dev/null; then
    echo "Error: yq not found. Install with: brew install yq" >&2
    exit 1
fi

if [[ ! -d "$DIST_DIR" ]]; then
    echo "Error: dist directory not found: $DIST_DIR" >&2
    exit 1
fi

if [[ ! -d "$THEMES_DIR" ]]; then
    echo "Error: themes directory not found: $THEMES_DIR" >&2
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
    persona_count=$(tar tzf "$pack" | grep "^original/.*\.png$" | wc -l | tr -d ' ')

    $first || printf ',\n'
    first=false
    printf '    "%s": {"pack_sha256": "%s", "pack_bytes": %s, "persona_count": %s}' \
        "$theme" "$sha" "$bytes" "$persona_count"
done
printf '\n  },\n'

# Personas section — parse theme YAMLs for role → filename-stem mapping
printf '  "personas": {\n'
first_theme=true
for theme in "${themes[@]}"; do
    yaml="$THEMES_DIR/${theme}.yaml"
    if [[ ! -f "$yaml" ]]; then
        echo "Warning: no theme YAML for $theme, skipping persona map" >&2
        continue
    fi

    $first_theme || printf ',\n'
    first_theme=false
    printf '    "%s": {' "$theme"

    first_role=true
    while IFS= read -r role; do
        [[ -z "$role" ]] && continue

        # Get shortName (fallback to character first word)
        short=$(yq -r ".agents.\"$role\".shortName // (.agents.\"$role\".character | split(\" \") | .[0])" "$yaml" 2>/dev/null)
        [[ -z "$short" || "$short" == "null" ]] && continue

        # Get OCEAN scores
        ocean_o=$(yq -r ".agents.\"$role\".ocean.O // empty" "$yaml" 2>/dev/null)
        [[ -z "$ocean_o" ]] && continue
        ocean_c=$(yq -r ".agents.\"$role\".ocean.C // empty" "$yaml" 2>/dev/null)
        ocean_e=$(yq -r ".agents.\"$role\".ocean.E // empty" "$yaml" 2>/dev/null)
        ocean_a=$(yq -r ".agents.\"$role\".ocean.A // empty" "$yaml" 2>/dev/null)
        ocean_n=$(yq -r ".agents.\"$role\".ocean.N // empty" "$yaml" 2>/dev/null)

        # Build slug: lowercase, non-alnum → dash, trim leading/trailing dashes
        slug=$(echo "$short" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g; s/^-*//; s/-*$//')
        stem="${slug}-${ocean_o}${ocean_c}${ocean_e}${ocean_a}${ocean_n}"

        $first_role || printf ', '
        first_role=false
        printf '"%s": "%s"' "$role" "$stem"
    done < <(yq -r '.agents | keys | .[]' "$yaml" 2>/dev/null)

    printf '}'
done
printf '\n  }\n'
printf '}\n'
