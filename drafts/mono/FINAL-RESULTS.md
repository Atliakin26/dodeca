# Picante Monomorphization Optimization - Final Results

## ✅ SUCCESS - Mission Accomplished

The trait object approach (`dyn ErasedCompute<DB>`) successfully reduced compile times by **36%** while maintaining correctness.

---

## Journey Summary

### Attempt 1: Generic DerivedCore (FAILED)
- Moved state machine to non-generic struct but kept generic method signature
- **Result:** Made it WORSE (337k IR lines, +43% more than baseline)
- **Why:** Method still generic over closure type `F` → still monomorphizes per query

### Attempt 2: Trait Objects (SUCCESS)
- Used `dyn ErasedCompute<DB>` trait object instead of generic closures
- **Result:** MASSIVE improvement (9k IR lines, -96% from baseline)
- **Why:** No generic type parameters in state machine = compiles once per DB type

---

## Final Measurements

### LLVM IR Reduction

| Metric | Baseline | After Dyndyn | Improvement |
|--------|----------|--------------|-------------|
| **State machine IR** | 234,836 lines | 9,368 lines | **-96%** ✅ |
| **Copies** | 50 | 2 | **-96%** ✅ |
| **% of total** | 12.0% | 0.6% | **-95%** ✅ |
| **Total IR** | 1,952,157 | 1,600,110 | -18% |

### Build Time Improvement

| Metric | Baseline | After Dyndyn | Improvement |
|--------|----------|--------------|-------------|
| **Wall-clock** | 25.53s | 16.29s | **-36%** ✅ |
| **CPU time** | 119.76s | 77.49s | **-35%** ✅ |
| **Parallelism** | 501% | 503% | Maintained |

---

## Validation ✅

### ✅ Sanity Check 1: Symbol Copies
```bash
$ grep -i "access_scoped" drafts/mono/dyndyn/llvm-lines-full.txt
9368 (0.6%, 24.3%)    2 (0.0%, 11.6%)  DerivedCore::access_scoped_erased::{{closure}}
```
- Exactly **2 copies** (DB + DatabaseSnapshot)
- No hidden monomorphization bloat

### ✅ Sanity Check 2: Behavioral Correctness
```bash
$ cargo test changed_at_stable_when_value_unchanged
test changed_at_stable_when_value_unchanged ... ok
```
- `changed_at` remains stable when values equal via `eq_erased`
- Prevents cascade invalidations
- No correctness-by-churn

---

## Implementation Details

### Core Architecture

**Before (failed attempt):**
```rust
async fn access_scoped_erased<DB, F>(
    compute: impl Fn() -> F  // ❌ Generic over F
) where F: Future<...>
```

**After (trait objects):**
```rust
async fn access_scoped_erased<DB>(
    compute: &dyn ErasedCompute<DB>,  // ✅ Trait object
    eq_erased: EqErasedFn,
)
```

### Key Components

1. **`trait ErasedCompute<DB>`** - Type-erased compute trait
   - Generic over `DB` only (not `K`, `V`, or closure types)
   - Returns `BoxFuture<'a, PicanteResult<ArcAny>>`

2. **`TypedCompute<DB, K, V>`** - Small per-query adapter
   - Implements `ErasedCompute<DB>`
   - Boxes futures and erases types
   - ~13k IR lines across 54 copies (thin wrappers - acceptable)

3. **`eq_erased_for<V>()`** - Deep equality helper
   - Function pointer (not generic in state machine)
   - Uses `facet_assert::check_same` for semantic equality
   - Prevents spurious invalidations

### Runtime Costs (Acceptable)

- ✅ One vtable dispatch per compute
- ✅ One BoxFuture allocation per compute
- ✅ Key decode per compute
- ✅ Deep equality check on recompute

Trade-off: Small runtime overhead for 36% faster builds.

---

## Test Coverage

All existing tests pass + new test added:

- ✅ `derived_caches_and_invalidates`
- ✅ `derived_singleflight_across_tasks`
- ✅ `detects_cycles_within_task`
- ✅ `persistence_roundtrip`
- ✅ `poisoned_cells_recompute_after_revision_bump`
- ✅ `changed_at_stable_when_value_unchanged` ⭐ NEW

---

## Files & Data

```
drafts/mono/
├── FINAL-RESULTS.md           # This file
├── before/                    # Baseline measurements
│   ├── README.md
│   ├── summary.txt
│   └── llvm-lines-full.txt
├── after/                     # Failed generic attempt
│   ├── README.md              # Documents why it failed
│   ├── summary.txt
│   └── llvm-lines-full.txt
└── dyndyn/                    # Successful trait object approach
    ├── README.md
    ├── VALIDATION.md          # Sanity check results
    ├── summary.txt
    └── llvm-lines-full.txt
```

---

## Commits

**Picante repository** (`feat/type-erasure-compile-time` branch):

1. `c88fd60` - Initial DerivedCore split (failed - regression)
2. `39c450d` - Trait object implementation (success)
3. `eee68e1` - Add changed_at stability test

**Dodeca repository:**
- Updated to use picante branch with trait objects
- All measurements captured in `drafts/mono/`

---

## Recommendation

**MERGE** the `feat/type-erasure-compile-time` branch:

✅ **Proven Results:**
- 96% reduction in state machine IR
- 36% faster clean builds
- All tests pass
- Behavioral correctness verified

✅ **Clean Implementation:**
- Well-documented code
- Clear separation of concerns
- Minimal runtime overhead

✅ **Validated:**
- Symbol copies confirmed (2 instead of 50)
- Deep equality working correctly
- No hidden bloat

This delivers exactly what was promised: dramatic compile-time improvement with acceptable runtime costs.
