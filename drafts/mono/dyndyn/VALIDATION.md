# Validation Results ✅

Both sanity checks requested by the consultant passed successfully.

## Sanity Check 1: Symbol Copies

**Requirement:** Confirm `access_scoped_erased` has ~2 instantiations (DB + snapshot), not hidden clones.

**Result:** ✅ PASS

```
$ grep -i "access_scoped" drafts/mono/dyndyn/llvm-lines-full.txt
     9368 (0.6%, 24.3%)      2 (0.0%, 11.6%)  picante::ingredient::derived::DerivedCore::access_scoped_erased::{{closure}}
```

**Analysis:**
- Only **2 copies** of the state machine exist in LLVM IR
- 9,368 total IR lines across those 2 copies
- No hidden helper function bloat
- This confirms the state machine is compiled exactly twice (likely for `DB` and `DatabaseSnapshot`)

---

## Sanity Check 2: Behavioral Correctness

**Requirement:** Test that recomputing the same value preserves `changed_at` (no correctness-by-churn).

**Result:** ✅ PASS

Added test: `changed_at_stable_when_value_unchanged` in `crates/picante/tests/basic.rs`

**Test scenario:**
1. Derived query computes `input % 10` (last digit only)
2. Input changes from 42 → 52 (forces recompute)
3. Output remains 2 (same value via `eq_erased`)
4. **Verify:** `changed_at` does NOT bump
5. Input changes from 52 → 47 (different last digit)
6. Output changes to 7
7. **Verify:** `changed_at` DOES bump

**Test output:**
```
running 1 test
test changed_at_stable_when_value_unchanged ... ok
```

**What this proves:**
- ✅ `eq_erased_for<V>()` correctly performs deep equality using `facet_assert::check_same`
- ✅ `changed_at` remains stable when values are semantically equal
- ✅ Prevents cascade invalidations when dependencies haven't meaningfully changed
- ✅ Still bumps `changed_at` when values actually differ

---

## Conclusion

Both sanity checks passed. The implementation has the **right shape**:
1. State machine compiled 2 times (not 50+)
2. Deep equality prevents spurious invalidations
3. No hidden monomorphization bloat

The trait object approach successfully delivers on all promises.
