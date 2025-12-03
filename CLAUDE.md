# Claude Code Instructions for dodeca

## Releasing

Use `knope release` to prepare and publish releases. Knope will:
- Bump version based on conventional commits
- Update CHANGELOG.md
- Commit changes
- Create and push git tag

Note: knope requires TTY for the Release step confirmation, so you may need to create and push the tag manually after knope prepares the release.

## Building

Use `cargo xtask build` to build everything (WASM, plugins, and dodeca).

## WASM Build Notes

The livereload-client is built with wasm-bindgen. The wasm-bindgen-cli version
must exactly match the wasm-bindgen library version in Cargo.lock. Check
`.github/build-setup.yml` and update the version there when updating
wasm-bindgen in Cargo.toml.
