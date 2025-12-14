# DerivedCore Refactoring - Measurement Results

## Executive Summary

**Status: REGRESSION** ⚠️

The refactoring to split DerivedIngredient into a non-generic DerivedCore struct **failed to reduce compile times** and actually made them worse.

## Key Findings

### LLVM IR Impact
```
BEFORE:  234,836 IR lines (12.0% of total) - 50 monomorphizations
AFTER:   337,041 IR lines (17.1% of total) - 73 monomorphizations

CHANGE:  +102,205 lines (+43.5%) | +23 copies (+46%)
```

### Build Time Impact
```
BEFORE:  25.53s wall-clock build time
AFTER:   26.71s wall-clock build time

CHANGE:  +1.18s (+4.6% slower)
```

## Why It Failed

The refactoring moved the state machine from:
```rust
impl<DB, K, V> DerivedIngredient<DB, K, V> {
    async fn access_scoped(...) { /* 300+ lines */ }
}
```

To:
```rust
struct DerivedCore { /* non-generic! */ }

impl DerivedCore {
    async fn access_scoped_erased<DB, F>(...) { /* 300+ lines */ }
}
```

**The critical mistake:** The method is still generic over `F` (the closure type), so the compiler monomorphizes it for every unique closure. We got MORE copies because the nested closure structure created more unique types.

## What We Learned

1. **Non-generic structs don't help if methods are generic** - Moving code to a non-generic type is useless if the methods accepting it remain generic over type parameters.

2. **Closure type erasure is necessary** - Each query creates a unique closure type when calling `access_scoped_erased<DB, F>()`. These closures differ in their captured types (K, V), so the compiler treats them as different F types.

3. **More indirection = more instantiations** - The refactored version has more complex nested closures (||async{...}), which created MORE unique closure types than the original implementation.

## What Would Actually Work

To compile the state machine once, we need **true type erasure** using trait objects:

```rust
impl DerivedCore {
    async fn access_scoped_erased(
        &self,
        db: &dyn IngredientLookup,  // ← trait object, not generic DB
        requested: DynKey,
        want_value: bool,
        compute_erased: Box<dyn Fn() -> BoxFuture<'_, ...>>,  // ← trait object, not generic F
    ) -> PicanteResult<ErasedAccessResult>
}
```

**Trade-offs:**
- ✅ Compiles once (1 copy instead of 50-73)
- ✅ Dramatic reduction in IR and compile time
- ❌ Runtime heap allocation (Box)
- ❌ Runtime dynamic dispatch overhead
- ❌ Complex lifetime management
- ❌ Harder to get right (borrow checker fights)

## Recommendation

**Revert this refactoring.** It makes compile times worse without any runtime benefit.

If we want to pursue compile-time optimization:
1. Prototype with trait objects to confirm it actually works
2. Measure runtime performance impact
3. Only merge if both compile-time AND runtime metrics are acceptable

## Data Location

- `before/` - Baseline measurements before refactoring
- `after/` - Measurements after DerivedCore split
- Both directories contain llvm-lines output, build times, and analysis

