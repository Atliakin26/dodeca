# Test Failures Investigation - Race Condition in Search Index Build

## Status
✅ **FIXED** - Implemented cell readiness handshake. All clippy warnings fixed and integration tests passing.

## Problem Summary
Integration tests hang/timeout because the server panics during startup with:
```
markdown parse_and_render plugin call failed: Transport(Encode(NoSlotAvailable))
thread '<unnamed>' panicked at crates/dodeca/src/queries.rs:393:10:
markdown plugin not loaded
error: Search index error: query panicked: markdown plugin not loaded
```

## Root Cause
The search index builder starts **before** the markdown cell's RPC dispatcher is fully initialized:

1. Server starts and prints `LISTENING_PORT` (tests successfully capture this)
2. Search index build begins immediately
3. Tries to call markdown cell RPC before cell is ready to handle requests
4. Gets `NoSlotAvailable` error → panics
5. Server crashes
6. Tests hang waiting for HTTP responses that will never come

## Timeline Evidence
From `/tmp/ddc-test-stderr.log`:
```
INFO loaded plugins from target/release
INFO HTTP cell connected via hub
LISTENING_PORT=46119          <-- Tests see this and think server is ready
...
[ddc-cell-markdown] registered as active peer
markdown plugin not loaded     <-- But cell not ready for RPC calls yet
```

## Code Locations

### Where search index builds (problematic)
- `src/main.rs:1060-1061` - `build_with_mini_tui()` spawns thread
- `src/main.rs:1355` - same in different code path
- `src/main.rs:2126` - `rebuild_search_for_serve()`

### Where cells register
- `src/cells.rs:651` - "loaded plugins from target/release"
- `src/cells.rs:333-345` - spawns plugins and waits for registration
- `src/cells.rs:455-468` - sets up RPC session and tracing dispatcher

### Registration vs Ready
The code waits for cells to **register** (line 651: "all 17 peers registered after 1.957647ms") but doesn't wait for them to be **ready to handle RPC calls**.

## Potential Fixes

### Option 1: Wait for Cell Ready Signal
Add a "ready" handshake after registration:
- Cell sends ready signal after dispatcher is set up
- Search index build waits for markdown cell ready
- Most robust but requires protocol change

### Option 2: Defer Search Index Build
Move search index build to happen lazily:
- On first HTTP request for search
- Or on a short timer after startup
- Quick fix but may affect first request latency

### Option 3: Retry Logic
Add retry/backoff when calling markdown cell:
- If `NoSlotAvailable`, wait 10ms and retry
- Fail after timeout (e.g. 5 seconds)
- Least invasive but feels hacky

### Option 4: Explicit Ordering
Ensure search index builds after a "cells ready" checkpoint:
- Add explicit synchronization point after all cells initialized
- Block search index build on this checkpoint
- Clear semantics but requires refactoring startup flow

## Recommended Approach
**Option 4** with a fallback to **Option 3** for robustness:

1. Add `plugins_ready()` function that returns when all cells are RPC-ready
2. Call it after the "loaded plugins" log line
3. Only then proceed to search index build
4. Add retry logic in `parse_and_render_markdown_cell()` as defensive measure

## Next Steps
1. Add tracing to see exactly when markdown cell dispatcher is ready
2. Verify timing: registration → dispatcher setup → first RPC call
3. Add synchronization point or defer search build appropriately
4. Test with integration tests to ensure race is fixed

## Related Files
- `crates/dodeca/src/main.rs` - startup sequence
- `crates/dodeca/src/cells.rs` - cell spawning and registration
- `crates/dodeca/src/search.rs` - search index build trigger
- `crates/dodeca/src/queries.rs:393` - panic location
- `crates/dodeca-integration/tests/serve.rs` - failing tests

## Test Command
```bash
cargo build --release && cargo test --release --package dodeca-integration --test serve -- test_all_pages_accessible
```

The `dodeca::serve_integration` test passes because it uses `CARGO_BIN_EXE_ddc` (test profile), while `dodeca-integration::serve` fails because it expects pre-built release binaries with all cells.

## Solution Implemented

Implemented **Option 4** (Explicit Ordering) with the following components:

### 1. Cell Lifecycle Protocol (`cells/cell-lifecycle-proto`)
- New RPC service `CellLifecycle` with `ready()` method
- Cells call `ready()` after starting their demux loop
- Proves the cell can handle RPC requests

### 2. Host-Side Registry (`crates/dodeca/src/cells.rs`)
- `CellReadyRegistry` tracks which cells have completed ready handshake
- Polling-based wait (works across different tokio runtimes)
- `wait_for_cell_ready()` and `wait_for_cells_ready()` APIs

### 3. Cell-Side Handshake (`crates/dodeca-cell-runtime`)
- Modified `run_cell_service()` to spawn demux loop in background
- Added yielding after spawn (critical for current_thread runtime)
- Calls ready handshake with retry/timeout
- Updated `cell-http` manually (doesn't use `run_cell_service`)

### 4. Startup Synchronization
- `LISTENING_PORT` output gated on http + markdown cells being ready
- Search index build waits for markdown cell before parsing
- Both use 5-10 second timeouts with fallback warnings

### Key Insights
- **Runtime isolation**: Search index builds in separate thread with own runtime
- **Polling vs Notify**: Must use polling (not `tokio::sync::Notify`) to work across runtimes
- **Yield requirement**: `current_thread` runtime needs explicit yields after spawning tasks
- **Deadlock prevention**: Cells must start demux loop before making RPC calls

### Test Results
✅ `test_all_pages_accessible` now passes consistently
✅ No more "markdown plugin not loaded" panics
✅ Proper synchronization between cell startup and work dispatch
