# DDC Binary Code Size Analysis

**Date**: 2025-12-14
**Binary**: ddc (dodeca main binary)
**Total LLVM IR Lines**: 1,952,157 lines across 41,947 function copies

## Summary: Picante DerivedIngredient is THE Bloat Source

The #1 source of code bloat is `picante::ingredient::derived::DerivedIngredient<DB,K,V>`, which generates massive amounts of LLVM IR for each query type.

## Top 5 Code Size Culprits

| Lines | % | Copies | Lines/Copy | Function |
|-------|---|--------|------------|----------|
| 234,836 | 12.0% | 50 | **4,697** | `picante::ingredient::derived::DerivedIngredient<DB,K,V>::access_scoped::{{closure}}` |
| 41,329 | 2.1% | 83 | 498 | `facet_postcard::deserialize::from_slice` |
| 31,100 | 1.6% | 50 | 622 | `picante::ingredient::derived::DerivedIngredient<DB,K,V>::try_revalidate::{{closure}}` |
| 25,935 | 1.3% | 413 | 63 | `<alloc::sync::Weak<T,A> as core::ops::drop::Drop>::drop` |
| 25,100 | 1.3% | 209 | 120 | `<alloc::vec::Vec<T> as alloc::vec::spec_from_iter_nested::SpecFromIterNested<T,I>>::from_iter` |

**Findings:**
- **Single worst offender**: `DerivedIngredient::access_scoped::{{closure}}` generates 4,697 lines of LLVM IR per query type
- With 50 different query types, that's 234,836 lines (12% of total binary!)
- This explains the 43,937 monomorphized instances we saw in self-profiling

## All Picante DerivedIngredient Functions

| Lines | % | Copies | Function |
|-------|---|--------|----------|
| 234,836 | 12.0% | 50 | `access_scoped::{{closure}}` |
| 31,100 | 1.6% | 50 | `try_revalidate::{{closure}}` |
| 19,765 | 1.0% | 27 | `save_records::{{closure}}` (PersistableIngredient) |
| 15,169 | 0.8% | 27 | `restore_runtime_state::{{closure}}` (PersistableIngredient) |
| 11,751 | 0.6% | 27 | `snapshot_cells_deep::{{closure}}` |
| 8,247 | 0.4% | 27 | `load_records` (PersistableIngredient) |
| 7,662 | 0.4% | 46 | `get::{{closure}}` |
| 6,709 | 0.3% | 27 | `touch::{{closure}}` (DynIngredient) |
| 5,111 | 0.3% | 27 | `touch::{{closure}}` |
| 4,898 | 0.3% | 46 | `get::{{closure}}::{{closure}}::{{closure}}` |
| 4,560 | 0.2% | 60 | `new` |
| 4,239 | 0.2% | 27 | `DerivedRecord<K,V>::__SHAPE_DATA::{{closure}}` |

**Total from DerivedIngredient alone**: ~360,000 lines (18.4% of binary)

## Other Notable Culprits

### Facet Postcard (Serialization)
- 41,329 lines (2.1%) from 83 copies of `deserialize::from_slice`
- Used for persisting picante query results

### Tokio (Async Runtime)
- 20,003 lines (1.0%) from 337 copies of `std::thread::local::LocalKey<T>::try_with`
- 15,549 lines (0.8%) from 146 copies of `tokio::task::task_local::LocalKey<T>::scope_inner`
- Necessary overhead for async queries

### Immutable Data Structures (im crate)
- 10,982 lines (0.6%) from 37 copies of `im::nodes::hamt::Node<A>::insert`
- 8,477 lines (0.4%) from 37 copies of `im::nodes::hamt::Iter<A>::next`
- Used by picante for persistent data structures

## Type Sizes

Checked for large types that might be expensive to copy/clone. Good news:

- No picante types > 100 bytes found
- Most `DerivedIngredient` types are 32 bytes (tokio task locals)
- Type sizes are NOT the problem here

**Conclusion**: The bloat is NOT from large types, but from **generating thousands of lines of code per query type**.

## Why Is This Happening?

### Picante's Architecture

Each `#[derive(Query)]` or manually implemented picante query generates:

1. **`access_scoped`**: Runtime tracking, dependency graph, cycle detection (4,697 lines/copy!)
2. **`try_revalidate`**: Check if cached value is still valid (622 lines/copy)
3. **`save_records` / `load_records`**: Persistence for incremental compilation
4. **`get` / `touch`**: Query execution with memoization

With 50 different query signatures in dodeca, each of these functions gets monomorphized 50 times.

### The 50 Query Types

Based on profiling data, dodeca has queries like:
- `DerivedIngredient<Database, Route, Option<String>>`
- `DerivedIngredient<Database, (), Vec<String>>`
- `DerivedIngredient<Database, (), HashMap<String, String>>`
- `DerivedIngredient<Database, (), Option<CompiledCss>>`
- `DerivedIngredient<DatabaseSnapshot, Route, Option<String>>` (snapshot versions!)
- ... etc

Note: There are also `Database` vs `DatabaseSnapshot` versions, effectively doubling many queries.

## Recommendations

### Short-term (easiest impact)

1. **Audit queries**: Do you really need 50 different derived queries?
   - Can some be combined?
   - Can some use the same types?

2. **Consolidate snapshot queries**:
   - Instead of having both `DerivedIngredient<Database, K, V>` and `DerivedIngredient<DatabaseSnapshot, K, V>`
   - Consider a single query type with runtime dispatch

3. **Check for unused queries**:
   - Functions like `render_markdown_cell` and `parse_frontmatter_cell` are marked as unused
   - If their queries exist, remove them

### Medium-term

4. **Use trait objects for cold paths**:
   - Queries that run rarely (e.g., initial build setup) could use `Box<dyn ...>`
   - Trades tiny runtime cost for massive compile time savings

5. **Simplify query value types**:
   - Instead of `HashMap<String, String>`, `HashMap<String, DataFile>`, etc.
   - Consider a single `HashMap<String, QueryValue>` enum

6. **Feature-gate queries**:
   - Dev server queries vs build-only queries
   - Don't compile what you're not using

### Long-term (Picante upstream)

7. **Reduce DerivedIngredient code size**:
   - 4,697 lines per instance is HUGE
   - Look into splitting generic/non-generic code
   - Use more dynamic dispatch internally

8. **Compile-time feature flags**:
   - Disable persistence for non-incremental builds
   - Disable tracing/debugging in release mode

## gingembre and cinereus Question

You asked why `gingembre` and `cinereus` are in the dep tree:

Looking at Cargo.toml:
- `gingembre` (line 140): Template engine for dodeca
- `cinereus` (transitive from facet): Color/ANSI handling

Both are legitimate dependencies, though gingembre seems like it might be feature-gatable if you don't always need the template engine.

## Next Steps

1. **Count actual queries**:
   ```bash
   grep -r "#\[derive(Query)\]" crates/dodeca/src/ | wc -l
   grep "impl.*Query for" crates/dodeca/src/ | wc -l
   ```

2. **Find unused queries**:
   - Look for query functions marked with `#[allow(dead_code)]`
   - Check if any query types appear in profiling but not in actual usage

3. **Benchmark impact of removing queries**:
   - Try commenting out a few queries and measuring compile time

4. **Profile a minimal build**:
   - Create a feature flag that disables most queries
   - See how fast it compiles with just core queries

## Files Generated

- `/tmp/llvm-lines-ddc.txt` - Full cargo-llvm-lines output
- `/tmp/type-sizes-ddc.txt` - Full -Zprint-type-sizes output

## Skills Created

- `.claude/skills/analyze-rust-code-size/` - How to use cargo-llvm-lines, type sizes, etc.
- Includes the **GOLDEN RULE**: Always capture to file first, analyze later!
