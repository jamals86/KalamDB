# Compilation Error Fix Workflow

**Command**: `/compile.fix`  
**Purpose**: Systematically analyze and fix all compilation errors

---

## Instructions for AI Agent

When the user runs `/compile.fix`, follow these steps:

### STEP 1: Capture All Errors
```powershell
# Run cargo check and save output to analysis file
cargo check --message-format=short 2>&1 | Out-File -FilePath "compilation_errors.txt" -Encoding UTF8
```

### STEP 2: Categorize Errors

Group errors into these categories (in priority order):

1. **CRITICAL ERRORS** 🔴 (Fix First)
   - Type errors (E0308, E0277, E0599, E0615)
   - Borrow checker errors (E0382, E0502, E0505)
   - Lifetime errors (E0106, E0621)
   - Missing trait implementations (E0277)
   - Pattern: `error[E...]`

2. **STRUCTURAL ISSUES** 🟠
   - Missing imports/modules (E0432, E0433)
   - Visibility errors (E0603, E0624)
   - Macro expansion errors
   - Pattern: `error[E4...]`

3. **UNUSED IMPORTS** ⚠️
   - Pattern: `warning: unused import`
   - Fix: Remove or comment out

4. **UNUSED VARIABLES** ⚠️
   - Pattern: `warning: unused variable`
   - Fix: Prefix with underscore (`_variable_name`)

5. **UNNECESSARY MUT** ⚠️
   - Pattern: `warning: variable does not need to be mutable`
   - Fix: Remove `mut` keyword

6. **DEPRECATION WARNINGS** ⚠️
   - Pattern: `warning: use of deprecated`
   - Fix: Replace with recommended alternative

### STEP 3: Create Error Analysis Document

Generate `error_analysis.md` with:
- Total error count by category
- Detailed breakdown with file paths and line numbers
- Root cause analysis for each error
- Fix strategy for each category
- Estimated fix time
- Recommended fix order

### STEP 4: Fix Errors Systematically

**Order of Operations**:

1. **Fix CRITICAL ERRORS first** (blocks compilation)
   - Start with widest-impact fixes (affects multiple files)
   - Then narrow fixes (single-file issues)
   - Use `replace_string_in_file` tool with 3-5 lines of context

2. **Fix STRUCTURAL ISSUES** (enables further compilation)
   - Import fixes (wide impact)
   - Module structure fixes
   - Visibility fixes

3. **Batch Fix WARNINGS** (code cleanliness)
   - Group similar warnings by file
   - Use regex or batch operations when possible
   - Unused imports: Remove entire lines
   - Unused variables: Add `_` prefix
   - Unnecessary mut: Remove `mut` keyword

4. **Address DEPRECATIONS** (future compatibility)
   - Follow compiler's recommended replacements
   - Update documentation references

### STEP 5: Verify Each Category

After fixing each category:
```powershell
# Re-run cargo check to see remaining errors
cargo check --message-format=short 2>&1 | Select-String -Pattern "error\[E" -Context 0,2

# Count remaining errors
cargo check 2>&1 | Select-String "error\[E" | Measure-Object
```

### STEP 6: Final Validation

```powershell
# Confirm zero errors
cargo check

# Run tests to ensure no regressions
cargo test --lib

# Check workspace-wide compilation
cargo check --workspace
```

---

## Fix Strategy Guidelines

### For Type Errors
1. Check field/method existence in struct definition
2. Verify correct type wrapping (Arc, Option, Result)
3. Look for recent renames or refactorings
4. Use `Arc::clone()` instead of `.clone()` for Arc types

### For Unused Code
1. **Imports**: Remove if truly unused, keep if used in tests/feature flags
2. **Variables**: Prefix with `_` if intentionally unused (e.g., destructuring, parsing)
3. **Mut**: Remove if variable is never reassigned

### For Batch Operations
- Group by file to minimize context switching
- Use consistent patterns (e.g., all `_` prefixes together)
- Verify each file compiles after batch edit

---

## Example Usage

**User says**: `/compile.fix`

**Agent does**:
1. Runs cargo check, saves to `compilation_errors.txt`
2. Analyzes and creates `error_analysis.md` with categories
3. Reports: "Found 1 error, 25 warnings in 4 categories"
4. Fixes Category 1 (CRITICAL) first
5. Verifies: "Error fixed, 25 warnings remain"
6. Asks: "Continue with warnings? (yes/no)"
7. If yes, batch-fixes warnings
8. Final verification: "✅ kalamdb-core compiles successfully: 0 errors, 0 warnings"

---

## Success Criteria

- ✅ `cargo check -p kalamdb-core` exits with code 0
- ✅ Zero compilation errors (`error[E...]`)
- ✅ Zero or minimal warnings (all intentional)
- ✅ All tests pass
- ✅ Error analysis document created for reference

---

## Notes for AI

- **Always prioritize compilation errors over warnings**
- **Use 3-5 lines of context** when using `replace_string_in_file`
- **Verify after each category** to catch cascading fixes
- **Don't assume** - read file contents before editing
- **Batch similar fixes** to minimize tool calls
- **Ask user** before fixing warnings if there are many (>20)

---

## Related Files

- `.github/copilot-instructions.md` - Project coding guidelines
- `AGENTS.md` - Development guidelines and architecture
- `specs/010-core-architecture-v2/spec.md` - Current phase specification
- `specs/010-core-architecture-v2/tasks.md` - Task tracking
