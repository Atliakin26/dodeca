#!/bin/bash
# Assemble release archive for a target
# Usage: scripts/assemble-archive.sh <target-triple>
# Example: scripts/assemble-archive.sh x86_64-unknown-linux-gnu
set -euo pipefail

TARGET="${1:?Usage: $0 <target-triple>}"

echo "Assembling archive for: $TARGET"

# Determine binary name and archive format
case "$TARGET" in
    *windows*)
        BINARY_NAME="ddc.exe"
        LIB_PREFIX=""
        LIB_EXT="dll"
        ARCHIVE_EXT="zip"
        ;;
    *apple*)
        BINARY_NAME="ddc"
        LIB_PREFIX="lib"
        LIB_EXT="dylib"
        ARCHIVE_EXT="tar.xz"
        ;;
    *)
        BINARY_NAME="ddc"
        LIB_PREFIX="lib"
        LIB_EXT="so"
        ARCHIVE_EXT="tar.xz"
        ;;
esac

ARCHIVE_NAME="dodeca-${TARGET}.${ARCHIVE_EXT}"

# Auto-discover plugins (crates with cdylib in Cargo.toml)
PLUGINS=()
for dir in crates/dodeca-*/; do
    if [[ -f "$dir/Cargo.toml" ]] && grep -q 'cdylib' "$dir/Cargo.toml"; then
        plugin=$(basename "$dir")
        # Convert crate name to lib name (dodeca-foo -> dodeca_foo)
        lib_name="${plugin//-/_}"
        PLUGINS+=("$lib_name")
    fi
done

echo "Discovered plugins: ${PLUGINS[*]}"

# Create staging directory
rm -rf staging
mkdir -p staging/plugins

# Copy binary
cp "target/${TARGET}/release/${BINARY_NAME}" staging/

# Copy plugins
for plugin in "${PLUGINS[@]}"; do
    PLUGIN_FILE="${LIB_PREFIX}${plugin}.${LIB_EXT}"
    SRC="target/${TARGET}/release/${PLUGIN_FILE}"
    if [[ -f "$SRC" ]]; then
        cp "$SRC" staging/plugins/
    else
        echo "Warning: Plugin not found: $SRC"
    fi
done

# Create archive
echo "Creating archive: $ARCHIVE_NAME"
if [[ "$ARCHIVE_EXT" == "zip" ]]; then
    cd staging && 7z a -tzip "../${ARCHIVE_NAME}" .
else
    tar -cJf "${ARCHIVE_NAME}" -C staging .
fi

# Cleanup
rm -rf staging

echo "Archive created: $ARCHIVE_NAME"
