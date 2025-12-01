#!/bin/bash
# Install file associations for Fulgur on Linux
# This script makes Fulgur appear in the "Open With" menu WITHOUT setting it as default

set -e

echo "Installing Fulgur to 'Open With' menu..."

# Determine if running as root
if [ "$EUID" -eq 0 ]; then
    DESKTOP_DIR="/usr/share/applications"
    SCOPE="system-wide"
else
    DESKTOP_DIR="$HOME/.local/share/applications"
    SCOPE="current user only"
fi

# Create directory if it doesn't exist
mkdir -p "$DESKTOP_DIR"

# Copy desktop file
if [ -f "resources/fulgur.desktop" ]; then
    cp resources/fulgur.desktop "$DESKTOP_DIR/"
    echo "✓ Copied desktop file to $DESKTOP_DIR"
else
    echo "✗ Error: resources/fulgur.desktop not found"
    exit 1
fi

# Update desktop database
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
    echo "✓ Updated desktop database"
fi

echo ""
echo "✓ Installation complete ($SCOPE)"
echo ""
echo "Fulgur is now available in the 'Open With' menu for:"
echo "  • Text files (.txt, .md, .rst)"
echo "  • Code files (.rs, .js, .ts, .py, .java, .go, etc.)"
echo "  • Web files (.html, .css, .scss, .vue, .svelte)"
echo "  • Config files (.json, .xml, .yaml, .toml, .ini)"
echo "  • And many more..."
echo ""
echo "To use Fulgur:"
echo "  1. Right-click any supported file"
echo "  2. Select 'Open With' → 'Fulgur'"
echo "  3. (Optional) Check 'Remember' to make it your preferred app"
echo ""
echo "Fulgur will NOT automatically become the default - you choose when to use it!"
