+++
title = "Installation"
description = "How to install dodeca"
weight = 10
+++

## macOS / Linux

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/bearcove/dodeca/releases/latest/download/dodeca-installer.sh | sh
```

## Windows

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/bearcove/dodeca/releases/latest/download/dodeca-installer.ps1 | iex"
```

## From source

Since dodeca uses a plugin architecture, building from source requires multiple steps:

```bash
git clone https://github.com/bearcove/dodeca.git
cd dodeca
cargo xtask build
```

This will build the WASM components, plugins, and the main dodeca binary.

## Verify

After installation, verify it works:

```bash
ddc --version
```

## Updating

To update an existing installation to the latest version:

```bash
ddc self-update
```
