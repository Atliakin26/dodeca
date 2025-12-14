# DYNDYN - Trait Object Implementation Results

## ✅ SUCCESS - Massive Compile-Time Improvement!

The trait object approach using `dyn ErasedCompute<DB>` achieved the expected compile-time improvements.

## Results Summary

### LLVM IR Reduction
| Metric | BEFORE | DYNDYN | Improvement |
|--------|--------|--------|-------------|
| State machine IR | 234,836 lines | 9,368 lines | **-96%** ✅ |
| Number of copies | 50 | 2 | **-96%** ✅ |
| % of total IR | 12.0% | 0.6% | **-95%** ✅ |

### Build Time Improvement
| Metric | BEFORE | DYNDYN | Improvement |
|--------|--------|--------|-------------|
| Wall-clock time | 25.53s | 16.29s | **-36%** ✅ |
| CPU time (user) | 119.76s | 77.49s | **-35%** ✅ |

### vs Failed Refactoring
| Metric | FAILED (generic F) | DYNDYN | Improvement |
|--------|-------------------|--------|-------------|
| State machine IR | 337,041 lines | 9,368 lines | **-97%** ✅ |
| Number of copies | 73 | 2 | **-97%** ✅ |
| Wall-clock time | 26.71s | 16.29s | **-39%** ✅ |

## Implementation Details

### What Changed

**Core state machine (DerivedCore):**
```rust
// Before (failed attempt)
async fn access_scoped_erased<DB, F>(
    compute_erased: impl Fn() -> F  // ❌ Still generic over F
) where F: Future<...>

// After (trait objects)
async fn access_scoped_erased<DB>(
    compute: &dyn ErasedCompute<DB>,  // ✅ Trait object, not generic
    eq_erased: EqErasedFn,
)
```

**Key insight:** The method signature is **only generic over `DB`**, not over closure/future types. This means it compiles **2 times** (for `DB` and `DatabaseSnapshot`) instead of 50+ times (once per query).

### Architecture

1. **`trait ErasedCompute<DB>`** - Trait for type-erased compute
   - Generic over `DB` but not `K` or `V`
   - Returns `BoxFuture<'a, PicanteResult<ArcAny>>`

2. **`TypedCompute<DB,K,V>`** - Small per-query adapter
   - Implements `ErasedCompute<DB>`
   - Boxes the future and erases the type
   - ~12k IR lines across 54 copies (acceptable - thin wrappers)

3. **`DerivedCore::access_scoped_erased<DB>`** - Monomorphic state machine
   - Takes `&dyn ErasedCompute<DB>` trait object
   - ~9k IR lines across **2 copies only!**
   - Uses function pointer `eq_erased` for deep equality

### Runtime Costs

These are acceptable trade-offs for 36% faster builds:
- ✅ One vtable dispatch per compute
- ✅ One `BoxFuture` allocation per compute
- ✅ Key decode per compute (was already happening)
- ✅ Deep equality check on recompute (was needed anyway)

## Files

- `llvm-lines-full.txt` - Complete LLVM IR analysis  
- `llvm-lines-derived-focus.txt` - DerivedIngredient functions only
- `build-time.txt` - Build timing log
- `summary.txt` - Detailed comparison

## Validation

All requirements met:
- ✅ Compiles (no errors)
- ✅ All tests pass
- ✅ 2 copies of state machine (DB + snapshot)
- ✅ 96% reduction in IR
- ✅ 36% faster build time

## Recommendation

**MERGE THIS!** This refactoring delivers exactly what was promised:
- Dramatic compile-time improvement
- Minimal runtime cost
- Clean, maintainable code
- All tests passing
