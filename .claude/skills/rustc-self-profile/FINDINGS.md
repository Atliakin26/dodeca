# Dodeca Compilation Analysis Findings

**Date**: 2025-12-14
**Subject**: Why compiling the `ddc` binary takes 26.55s out of 38s total build time

## Summary

The ddc binary compilation is dominated by **LLVM backend work** (79.5s out of 139.5s total), with significant but not excessive monomorphization (43,937 instances totaling 3.84s).

## Detailed Breakdown

### Time Distribution (Total: ~139s)

| Phase | Time | % | Details |
|-------|------|---|---------|
| **LLVM Backend** | 99.3s | 71.2% | Code generation, optimization, LTO |
| **Codegen** | 10.3s | 7.4% | Finish codegen, linker |
| **Monomorphization** | 3.8s | 2.7% | 43,937 generic instances |
| **Type Checking** | 3.0s | 2.2% | Typeck, MIR, borrow checking |
| **Trait Resolution** | 2.6s | 1.9% | Proving predicates, trait selection |
| **Layout Computation** | 2.3s | 1.6% | 132,526 layout_of calls |
| **Other** | 17.7s | 12.7% | Parsing, macro expansion, etc. |

### LLVM Backend (99.3s)

```
LLVM_module_codegen_emit_obj:  32.42s (23.2%)
LLVM_module_optimize:          27.19s (19.5%)
LLVM_lto_optimize:             19.98s (14.3%)
finish_ongoing_codegen:        10.26s (7.4%)
LLVM_thinlto:                   9.39s (6.7%)
LLVM_thin_lto_import:           8.82s (6.3%)
```

This is where **most** of the time goes. The compiler is generating a lot of code and spending significant time optimizing it.

### Monomorphization (3.8s, 43,937 instances)

```
items_of_instance: 3.84s total, 43,937 invocations, 0.087ms average
```

43,937 monomorphized instances is **high** but not catastrophic. For comparison:
- Picante itself: 1,447 instances (modest)
- The explosion happens when picante's generic types are used with many different concrete types in dodeca

This suggests a **trait/generic explosion** from using picante traits across many types.

### Trait Resolution (2.6s)

```
codegen_select_candidate:    1.51s (19,640 calls)
type_op_prove_predicate:     1.10s (20,370 calls)
```

Not excessive, but noticeable. Trait selection is occurring frequently but individual operations are fast.

### Type Checking (3.0s)

```
typeck:      1.01s (3,899 functions)
mir_borrowck: 2.61s (2,601 functions)
```

Normal for a codebase of this size.

## Key Findings

### 1. LLVM Backend is the Main Bottleneck (71%)

**Why**: The compiler is generating and optimizing a lot of code. This includes:
- 256 codegen units
- Link-time optimization (LTO)
- Full optimization passes

**Impact**: This is somewhat expected but could be reduced.

### 2. Moderate Monomorphization Pressure (43k instances)

**Why**: Picante provides generic traits that get instantiated for many different types throughout dodeca.

**Comparison**:
- Picante crate itself: 1,447 instances
- ddc binary using picante: 43,937 instances (30x explosion)

This is the "trait explosion" you suspected.

### 3. Not a Complete Disaster

While 43k instances seems high, the average time per instance is only 0.087ms. The total monomorphization time (3.8s) is only 2.7% of the total compile time.

## Recommendations

### Short-term Optimizations (Fastest Impact)

1. **Reduce LTO aggressiveness**
   ```toml
   [profile.dev]
   lto = false  # or "thin"

   [profile.release]
   lto = "thin"  # instead of "fat"
   ```

2. **Increase codegen-units** (trades runtime perf for compile time)
   ```toml
   [profile.dev]
   codegen-units = 256  # default is 256, can confirm

   [profile.release]
   codegen-units = 16  # instead of 1
   ```

3. **Lower optimization level for dev builds**
   ```toml
   [profile.dev]
   opt-level = 1  # or 0 for fastest
   ```

### Medium-term (Reduce Monomorphization)

4. **Profile which types cause the most instances**
   ```bash
   cargo +nightly rustc --bin ddc -- -Z print-mono-items=eager 2>&1 | \
     grep "picante" | \
     sort | uniq -c | sort -rn | head -50
   ```

   This will show you which picante types/traits are being instantiated most.

5. **Consider `dyn Trait` for some cases**
   - If you have a trait like `picante::Request` being used with 100 different types
   - Consider using `Box<dyn Request>` or `&dyn Request` instead
   - This creates **one** vtable implementation instead of 100 monomorphized versions
   - Trade-off: slight runtime overhead for faster compilation

6. **Consolidate similar types**
   - If you have many types that are almost identical (e.g., different cell types)
   - Consider using a single enum or struct with type parameters

### Long-term (Architecture)

7. **Separate compilation units**
   - Split the binary into smaller pieces
   - Use dynamic linking for plugins/cells
   - Each piece compiles faster independently

8. **Lazy compilation**
   - Only compile features/cells that are actually used
   - Feature-gate expensive generic instantiations

9. **Check for unintended trait bounds**
   ```bash
   # Find where clause explosion
   rg "where\s" crates/dodeca/src/ -A 5 | less
   ```

## Comparison: Picante vs ddc

| Metric | Picante Crate | ddc Binary |
|--------|---------------|------------|
| Total compile time | ~3s | ~40s (just ddc crate) |
| items_of_instance | 1,447 | 43,937 |
| Monomorphization time | 50.93ms | 3.84s |

**Conclusion**: Picante itself is not slow to compile. The issue is that dodeca uses picante traits with many different types, causing a 30x explosion in monomorphized instances.

## What to Investigate Next

1. **Which types are being monomorphized most?**
   - Run: `cargo +nightly rustc --bin ddc -- -Z print-mono-items=eager`
   - Look for patterns like `picante::Request<CellA>`, `picante::Request<CellB>`, etc.

2. **Are there unnecessary trait bounds?**
   - Check `where` clauses for overly generic constraints
   - Look for traits that require many other traits

3. **Can some traits use dynamic dispatch?**
   - Identify hot paths vs cold paths
   - Use `dyn Trait` for cold paths, generics for hot paths

4. **Are there large types?**
   ```bash
   cargo +nightly rustc --bin ddc -- -Z print-type-sizes 2>&1 | head -100
   ```
   Large types get expensive to move around and duplicate across monomorphizations.

## Tools Created

I've created two skills for future analysis:

1. `.claude/skills/rustc-self-profile/` - How to profile Rust compilation
2. `.claude/skills/analyze-compile-times-duckdb/` - DuckDB queries for analysis

Use these for ongoing compile time investigation.
