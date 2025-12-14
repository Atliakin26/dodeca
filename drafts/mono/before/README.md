# BEFORE Measurements - Baseline for DerivedIngredient Refactoring

This directory contains baseline measurements of dodeca's compile time and LLVM IR generation BEFORE the DerivedCore refactoring.

## Key Metrics

### LLVM IR Generation (cargo-llvm-lines)
- **Total IR:** 1,952,157 lines across 41,947 function copies
- **DerivedIngredient state machine:** 234,836 lines (12% of total!) across **50 copies**
- **Problem:** `access_scoped::{{closure}}` is monomorphized 50 times for different (DB,K,V) combinations

### Build Time
- **Clean build:** 25.53s wall-clock time
- **CPU time:** 119.76s user + 8.60s system (~5x parallelism)
- **LLVM backend:** 79.2s total (codegen 32.2s + optimize 27.2s + LTO 19.8s)

## Files

### Summary
- `summary.txt` - Quick overview of key findings and expected improvements

### LLVM IR Analysis
- `llvm-lines-full.txt` - Complete output from cargo-llvm-lines (top LLVM IR generators)
- `llvm-lines-derived-focus.txt` - Filtered view of DerivedIngredient functions only

### Build Time Analysis
- `build-time.txt` - Log of clean cargo build with timing
- `build-time-profile.txt` - Log of build with -Zself-profile enabled
- `profile-summary.txt` - Processed self-profile data showing compile-time hotspots
- `profile/*.mm_profdata` - Raw self-profile data files (binary format)

## Expected Improvement

After the refactoring:
- `access_scoped_erased` should appear as **1-2 copies** (generic over DB only) instead of 50
- LLVM IR should drop from 234,836 lines to ~4,700 lines (50x reduction)
- Build time should decrease proportionally to reduced IR (exact savings TBD)

## How to Compare

After updating dodeca to use refactored picante, run the same measurements in `drafts/mono/after/` and compare:

```bash
# Compare LLVM IR
diff -u before/llvm-lines-derived-focus.txt after/llvm-lines-derived-focus.txt

# Compare build times
grep "Finished" before/build-time.txt after/build-time.txt

# Compare total IR
grep "TOTAL" before/llvm-lines-full.txt after/llvm-lines-full.txt
```
