+++
title = "Plugins"
description = "Plugin systems for extending dodeca functionality"
weight = 50
+++

## Introduction

Dodeca uses plugins to keep the core binary small and fast to link. Heavy dependencies live in plugins, which compile and link independently.

There are currently two plugin systems:

| System | Type | Communication | Status |
|--------|------|---------------|--------|
| **Plugcard** | Dynamic library (.so/.dylib/.dll) | Serialized method calls | Legacy, being phased out |
| **Rapace** | Subprocess binary | Unix socket RPC | Active development |

All plugins are being migrated to **rapace**.

---

## Plugcard (Legacy)

A dynamic library plugin system inspired by [postcard-rpc](https://github.com/jamesmunns/postcard-rpc).

Plugcard plugins are dynamic libraries loaded into the main process. They use serialized method calls across the FFI boundary.

### How It Works

The `#[plugcard]` attribute macro transforms your function into a plugin method:

1. **Input struct** - Arguments are bundled into a generated struct with serde derives
2. **FFI wrapper** - An `extern "C"` function that deserializes input, calls your function, serializes output
3. **Registration** - A static `MethodSignature` is registered in a distributed slice

### Quick Start

Add dependencies:

```toml
[dependencies]
plugcard = { path = "../plugcard" }
linkme = "0.3"
postcard-schema = { version = "0.1", features = ["derive", "alloc"] }

[lib]
crate-type = ["cdylib", "rlib"]
```

Mark functions with `#[plugcard]`:

```rust,noexec
use plugcard::plugcard;

#[plugcard]
pub fn reverse_string(input: String) -> String {
    input.chars().rev().collect()
}

#[plugcard]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The macro generates all FFI wrappers and registration code.

### Generated Code

For a function like:

```rust,noexec
#[plugcard]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The macro generates:

```rust,noexec
// Original function preserved
pub fn add(a: i32, b: i32) -> i32 { a + b }

// Input composite type
#[derive(Serialize, Deserialize, Schema)]
struct __PlugcardInput_add { pub a: i32, pub b: i32 }

// C-compatible wrapper
unsafe extern "C" fn __plugcard_wrapper_add(data: *mut MethodCallData) {
    // Deserialize input
    let input: __PlugcardInput_add = postcard::from_bytes(...)?;
    // Call function
    let result = add(input.a, input.b);
    // Serialize output
    postcard::to_slice(&result, ...)?;
}

// Auto-register in distributed slice
#[distributed_slice(METHODS)]
static __PLUGCARD_SIG_add: MethodSignature = MethodSignature {
    key: compute_key("add", ...),
    name: "add",
    input_schema: ...,
    output_schema: ...,
    call: __plugcard_wrapper_add,
};
```

### API Reference

#### `MethodSignature`

```rust,noexec
pub struct MethodSignature {
    pub key: u64,           // Unique key from name + schemas
    pub name: &'static str, // Human-readable method name
    pub input_schema: &'static NamedType,
    pub output_schema: &'static NamedType,
    pub call: unsafe extern "C" fn(*mut MethodCallData),
}
```

#### `MethodCallData`

The FFI boundary structure:

```rust,noexec
#[repr(C)]
pub struct MethodCallData {
    pub key: u64,
    pub input_ptr: *const u8,
    pub input_len: usize,
    pub output_ptr: *mut u8,
    pub output_cap: usize,
    pub output_len: usize,  // Set by callee
    pub result: MethodCallResult,
}
```

#### `MethodCallResult`

```rust,noexec
#[repr(C)]
pub enum MethodCallResult {
    Success,
    DeserializeError,
    SerializeError,
    MethodError,
    UnknownMethod,
}
```

### Method Keys

Method keys are computed at compile time using FNV-1a hash of:
- Method name
- Input schema name
- Output schema name

This ensures type-safe dispatch: if schemas change, keys change.

### Crate Structure

- **plugcard** - Core types and runtime
- **plugcard-macros** - The `#[plugcard]` proc macro (uses [unsynn](https://docs.rs/unsynn) for parsing)
- **dodeca-baseline** - Example plugin with test functions

---

## Rapace (Recommended)

Rapace plugins are standalone executables that communicate with the host via Unix socket RPC using the [rapace](https://github.com/bearcove/rapace) framework.

### Benefits

- **Process isolation** - Plugins run in separate processes, improving stability
- **Language flexibility** - Any language that can speak the protocol works
- **Async support** - Full async/await with independent runtimes per plugin
- **Hot reload potential** - Easier to restart individual plugins

### Current Rapace Plugins

- `dodeca-mod-http` - HTTP dev server with WebSocket support for live reload

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Core dodeca                             │
│  ┌─────────┐ ┌─────────┐ ┌──────────┐ ┌─────────────────┐   │
│  │  Salsa  │ │Markdown │ │ Template │ │  Plugin Host    │   │
│  │(queries)│ │ Parser  │ │  Engine  │ │ (rapace server) │   │
│  └─────────┘ └─────────┘ └──────────┘ └────────┬────────┘   │
└────────────────────────────────────────────────┼────────────┘
                                                 │ Unix socket
                                                 ▼
                              ┌─────────────────────────────┐
                              │    dodeca-mod-http          │
                              │   (axum HTTP server)        │
                              │   - serves HTTP requests    │
                              │   - WebSocket for devtools  │
                              │   - calls back to host      │
                              └─────────────────────────────┘
```

### Communication Flow

The plugin connects to the host via Unix socket and makes RPC calls:

```
Plugin (dodeca-mod-http)              Host (dodeca)
         │                                  │
         │── connect to socket ────────────▶│
         │                                  │
         │── find_content("/foo") ─────────▶│
         │                                  │ (queries Salsa DB)
         │◀── ServeContent::Html {...} ─────│
         │                                  │
         │── open_ws_tunnel() ─────────────▶│
         │                                  │ (creates tunnel)
         │◀── tunnel_id ────────────────────│
         │                                  │
```

### Protocol Definition

Rapace plugins use trait-based protocol definitions with the `#[rapace::service]` macro:

```rust,noexec
#[rapace::service]
pub trait ContentService {
    async fn find_content(&self, path: String) -> ServeContent;
    async fn get_scope(&self, route: String, path: Vec<String>) -> Vec<ScopeEntry>;
    async fn eval_expression(&self, route: String, expression: String) -> EvalResult;
    async fn open_ws_tunnel(&self) -> u64;
}
```

The macro generates:
- Client types for making RPC calls
- Server types for handling RPC calls
- Serialization/deserialization code

### Creating a Rapace Plugin

1. Define the protocol in a shared crate (e.g., `dodeca-serve-protocol`)
2. Implement the server side in the host
3. Create the plugin binary that connects and uses the client

See `crates/dodeca-mod-http/` for a complete example.

---

## Why Plugins?

The primary motivation is **link speed**. Rust's incremental compilation is fast, but linking a large binary with many dependencies is slow. By moving functionality into plugins:

- The main `dodeca` binary stays small and links fast
- Plugins compile and link independently
- Changing a plugin doesn't require relinking the main binary
- Heavy dependencies (image processing, font subsetting, HTTP) live in plugins

This dramatically improves iteration speed during development.

## Future Plans

More functionality will move to rapace plugins:

- `http-client` - For link checking and external fetches
- `search` - Full-text search indexing (replacing pagefind)
- `image-processing` - Image optimization and conversion
- `font-subsetting` - Web font optimization

Plugins can depend on each other through the message-passing system, keeping each focused and avoiding duplicated dependencies.
