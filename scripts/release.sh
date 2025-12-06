#!/bin/bash
# Prepare release artifacts
# Usage: scripts/release.sh
# Run from repo root after downloading all build artifacts to dist/
set -euo pipefail

echo "Preparing release..."

# Copy shell installer
cp install.sh dist/dodeca-installer.sh
echo "Copied: dodeca-installer.sh"

# Generate PowerShell installer
cargo xtask generate-ps1-installer dist/dodeca-installer.ps1
echo "Generated: dodeca-installer.ps1"

# List all artifacts
echo ""
echo "Release artifacts:"
find dist -type f | sort

# Generate checksums
echo ""
echo "Generating checksums..."
cd dist
sha256sum * > SHA256SUMS
cat SHA256SUMS
