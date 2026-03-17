#!/bin/bash
# Fetch the latest versions of bundled themes from the gpui-component repository.
# Only syncs the themes already included in Fulgur — does not add new ones.

set -euo pipefail

THEMES_URL="https://raw.githubusercontent.com/longbridge/gpui-component/main/themes"
THEMES_DIR="$(dirname "$0")/../src/themes"

THEMES=(
    catppuccin
    everforest
    hybrid
    macos-classic
    mellifluous
    solarized
    spaceduck
    tokyonight
)

for theme in "${THEMES[@]}"; do
    echo "Syncing $theme.json..."
    curl -sSf "$THEMES_URL/$theme.json" -o "$THEMES_DIR/$theme.json"
done

echo "Done."
