# Phase 012 - What to Focus On Next

## 🎯 The 3 Main Things from Your Spec

You asked me to check what's still relevant from the spec. Here are **the 3 main goals you listed**:

### 1. AS USER Syntax ❌ NOT DONE
**What it is**: Allow service accounts to execute DML on behalf of users
```sql
INSERT INTO messages AS USER 'alice' VALUES (...)
UPDATE documents AS USER 'bob' SET status = 'approved' WHERE id = 123
DELETE FROM events AS USER 'charlie' WHERE event_id = 456
```

**Why you need it**: Multi-tenant systems where services act on behalf of users (chat, notifications, AI)

**Status**: **NOT STARTED** - 26 tasks remaining (T151-T176 in tasks.md)

**Complexity**: Medium - SQL parser + auth validation + audit logging

---

### 2. Manifest Files ❌ NOT DONE
**What it is**: Track metadata about Parquet batch files for efficient querying

**Why you need it**: Performance - without manifests, queries scan ALL Parquet files even when most are irrelevant

**Status**: **NOT STARTED** - 60 tasks in two parts:
- User Story 6 (Manifest Cache): 33 tasks (T069-T101)
- User Story 2 (Manifest Optimization): 27 tasks (T102-T134)

**Complexity**: High - RocksDB CF, caching service, query planner integration

---

### 3. Full DML Support (User/Shared Tables) ⚠️ PARTIALLY DONE
**What it is**: UPDATE and DELETE should work on ALL data (hot storage AND Parquet files)

**Current State**:
- ✅ Works on **hot storage** (RocksDB)
- ❌ Broken on **cold storage** (Parquet files that have been flushed)

**Problem**: If you flush data to Parquet, then try to UPDATE/DELETE it, the operation won't work because it only scans RocksDB

**Status**: **PARTIAL** - ~15 tasks remaining to merge hot+cold storage

**Complexity**: Medium - Extend existing scan logic to include Parquet

---

## ✅ What's Already Done (Don't Worry About These)

### MVCC Architecture (User Story 5) ✅
- SeqId versioning system
- Unified DML functions
- System columns (_seq, _deleted)
- ~800 lines of duplicate code eliminated

### Provider Consolidation (User Story 9) ✅
- Generic BaseTableProvider<K, V> trait
- Unified architecture for User/Shared/Stream tables
- ~1200 lines eliminated

### Crate Reorganization ✅
- Cleaner project structure
- 11 → 9 crates

---

## ❌ What's NOT Relevant (Ignore These)

1. **SystemColumnsService** mentions - Design changed, now use MVCC instead
2. **_id and _updated columns** - We use `_seq` (Snowflake ID) instead
3. **Phase 2.5 tasks** (T073-T082) - Superseded by Phase 13

---

## 💡 My Recommendation

**Start with**: **Complete Full DML Support** (~15 tasks)
- **Why**: Core functionality that's 80% done
- **Impact**: Makes UPDATE/DELETE work on flushed data
- **Effort**: Low - extends existing MVCC work
- **Tasks**: T046-T049 (flush), T106-T109 (UPDATE/DELETE with Parquet merge)

**Then do**: **AS USER** (26 tasks)
- **Why**: Explicit spec requirement, critical for multi-tenant
- **Impact**: Enables service account impersonation
- **Effort**: Medium - new feature area
- **Tasks**: T151-T176

**Finally**: **Manifest Files** (60 tasks)
- **Why**: Performance optimization
- **Impact**: Massive speedup for large tables
- **Effort**: High - substantial infrastructure
- **Tasks**: T069-T134

---

## 🚫 Can Be Deferred

- **Bloom Filters** (User Story 3) - Needs manifests first
- **Centralized Config** (User Story 7) - Nice to have
- **Type-Safe Job Params** (User Story 8) - Nice to have
- **Performance Tests** (Phase 10) - Do last

---

## 📄 Updated Documents

I've updated these files for you:

1. **STATUS-2025-11-14.md** - Full status review with all details
2. **spec.md** - Added ✅/⚠️/❌ markers to each user story
3. **tasks.md** - Added progress summary table
4. **SUMMARY.md** - Quick overview

All documents are in: `/Users/jamal/git/KalamDB/specs/012-full-dml-support/`

---

## ❓ Questions for You

1. **Which is most important for your current needs?**
   - Full DML on Parquet (finish what's 80% done)
   - AS USER impersonation (new feature)
   - Manifest files (performance)

2. **What's your use case for AS USER?**
   - If you don't need service account impersonation, we can skip it

3. **How big are your tables?**
   - If tables are small, manifests are less critical
   - If tables are large (100K+ rows), manifests are essential

Let me know what you'd like to focus on and I can help you continue!
