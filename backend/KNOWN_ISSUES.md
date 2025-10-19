# Known Issues

## Compilation Error: Arrow 52.2.0 + Chrono 0.4.40+ Conflict

**Status**: ✅ **RESOLVED** (via chrono pin)  
**Affects**: All phases after Phase 1.5  
**Severity**: High  
**Upstream Issue**: Yes

### Problem

Builds fail with:

```
error[E0034]: multiple applicable items in scope
  --> arrow-arith-52.2.0/src/temporal.rs:90:36
90 |         DatePart::Quarter => |d| d.quarter() as i32,
   |                                    ^^^^^^^ multiple `quarter` found
```

### Cause

`chrono v0.4.40+` added `Datelike::quarter()` which collides with Arrow's `ChronoDateExt::quarter()` in **arrow-arith 52.2.0**. The Arrow team fixed this upstream (PR #7198) and published hotfixes in **54.2.1/54.3.0**; **no 52.x backport exists**. 

**Reference**: https://github.com/apache/arrow-rs/issues/7196

### Current Solution (Applied)

**Pin Chrono to 0.4.39**

We've pinned `chrono = "=0.4.39"` in `workspace.dependencies` to avoid the conflicting `Datelike::quarter()` method that was introduced in 0.4.40.

To verify the fix is working:

```bash
# Update the lockfile to enforce the pin across all dependencies
cargo update -p chrono --precise 0.4.39

# Verify only one chrono version exists in the tree
cargo tree -i chrono -d
```

**Reference**: https://github.com/apache/arrow-rs/issues/7209

### Alternative Solutions (Future Options)

1. **Upgrade to DataFusion ≥ 45.0.0 (recommended long-term):**
   DF 45 depends on Arrow **54.x**, which includes the fix/hotfix. Some minor API updates may be required.
   
   **Reference**: https://github.com/apache/datafusion/issues/14114

2. **If upgrading or pinning is impossible:** 
   Fork `arrow-arith 52.2.0` and change the call to fully qualified syntax to avoid ambiguity, then use `[patch.crates-io]` to point `arrow-arith` to the fork (same `52.2.0` version).

### Not Recommended / Notes

* Waiting for **Arrow 52.3.x** isn't actionable (no active 52.x maintenance).
* Downgrading DF to 39/38 still lands on Arrow 52.x in practice; the conflict persists if chrono 0.4.40+ is present.
* `[patch.crates-io]` cannot swap crates.io versions with other crates.io versions; use `cargo update -p <crate> --precise <ver>` to pin, or patch to a **git** fork.

**Reference**: https://users.rust-lang.org/t/overriding-crates-io-version-for-all-crates/115428

### Impact on Phase 2

Unchanged: **kalamdb-sql** can proceed (RocksDB, serde, sqlparser-rs only).

### Migration Plan

When ready to upgrade DataFusion (Phase 3+):
1. Upgrade to DataFusion ≥ 45.0.0
2. Remove the `=0.4.39` pin and use `"0.4"` or latest stable
3. Address any DataFusion API changes
4. Test thoroughly

### Last Updated

2025-10-17
