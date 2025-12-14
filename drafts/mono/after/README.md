# AFTER Measurements - DerivedCore Refactoring Results

## ⚠️ REGRESSION DETECTED

The refactoring **made compile times WORSE**, not better.

## Comparison

### LLVM IR Generation
| Metric | BEFORE | AFTER | Change |
|--------|--------|-------|--------|
| State machine IR lines | 234,836 (12.0%) | 337,041 (17.1%) | **+102,205 (+43.5%)** ⚠️ |
| Number of copies | 50 | 73 | **+23 (+46%)** ⚠️ |
| Total IR lines | 1,952,157 | 1,967,782 | +15,625 (+0.8%) |

### Build Time
| Metric | BEFORE | AFTER | Change |
|--------|--------|-------|--------|
| Wall-clock time | 25.53s | 26.71s | **+1.18s (+4.6%)** ⚠️ |
| CPU time (user) | 119.76s | 105.34s | -14.42s (-12%) ✓ |
| Parallelism | 501% | 415% | -86% (-17%) ⚠️ |

## Root Cause

The refactoring split `DerivedIngredient<DB,K,V>` into:
- `DerivedCore` (non-generic struct)
- `DerivedCore::access_scoped_erased<DB, F>()` (generic method)

**The problem:** The method is still generic over `F` (the closure/future type), so Rust monomorphizes it for every unique closure type. We actually got MORE instantiations (73 vs 50) because:

1. The nested closure structure is more complex
2. Each `(DB, K, V)` combination creates different closure types
3. The compiler can't optimize away the extra indirection

## What Didn't Work

Simply moving code from `impl<DB,K,V>` to `impl DerivedCore` does NOT reduce monomorphization if the methods remain generic over type parameters.

## What Would Work

To truly compile the state machine once, we need to erase the `F` type parameter using trait objects:

```rust
async fn access_scoped_erased(
    &self,
    db: &dyn IngredientLookup,
    requested: DynKey,
    want_value: bool,
    compute_erased: Box<dyn Fn() -> BoxFuture<'_, PicanteResult<Arc<dyn Any + Send + Sync>>>>,
) -> PicanteResult<ErasedAccessResult>
```

But this introduces:
- Heap allocation overhead (Box)
- Dynamic dispatch overhead (dyn)
- Complex lifetime management ('_ in BoxFuture)
- Potential performance regression at runtime

## Files

- `llvm-lines-full.txt` - Complete LLVM IR analysis
- `llvm-lines-derived-focus.txt` - DerivedIngredient/DerivedCore functions only
- `build-time.txt` - Build log with timing
- `summary.txt` - Detailed comparison and analysis

## Recommendation

This refactoring should be **reverted** or **redesigned**. The current approach:
- ❌ Increases IR by 43%
- ❌ Increases monomorphization by 46%
- ❌ Slows build time by 4.6%
- ❌ Reduces parallelism by 17%

A different approach is needed to actually reduce compile times.
