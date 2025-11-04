#!/bin/bash
# Fix Phase 5 test compatibility issues

# Files to fix (standalone test files, not integration tests)
FILES=(
    "backend/tests/test_password_complexity.rs"
    "backend/tests/test_user_sql_commands.rs"
)

for file in "${FILES[@]}"; do
    echo "Fixing $file..."
    
    # Fix service constructors (remove arguments)
    sed -i.bak 's/UserTableService::new([^)]*)/UserTableService::new()/g' "$file"
    sed -i.bak 's/SharedTableService::new([^)]*)/SharedTableService::new()/g' "$file"
    sed -i.bak 's/StreamTableService::new([^)]*)/StreamTableService::new()/g' "$file"
    sed -i.bak 's/TableDeletionService::new([^)]*)/TableDeletionService::new()/g' "$file"
    
    # Fix SqlExecutor::new (remove session_context parameter)
    # This is trickier - need multi-line replacement
    perl -i.bak2 -0777 -pe 's/SqlExecutor::new\(\s*namespace_service,\s*session_context,\s*user_table_service,\s*shared_table_service,\s*stream_table_service,\s*\)/SqlExecutor::new(\n        namespace_service,\n        user_table_service,\n        shared_table_service,\n        stream_table_service,\n    )/gs' "$file"
    
    # Update setup function return signature to include session_ctx
    sed -i.bak3 's/(executor, temp_dir, kalam_sql)/(executor, temp_dir, kalam_sql, session_ctx)/g' "$file"
    sed -i.bak4 's/(executor, _temp_dir, kalam_sql)/(executor, _temp_dir, kalam_sql, session_ctx)/g' "$file"
    
    # Add session_ctx to executor.execute() calls - this needs careful handling
    # Pattern: .execute("...", Some(&...)) -> .execute(&session_ctx, "...", Some(&...))
    # Pattern: .execute(sql, Some(&...)) -> .execute(&session_ctx, sql, Some(&...))
    
    echo "  ✓ Fixed service constructors and SqlExecutor::new"
    echo "  ⚠ Manual fixes needed for .execute() calls - add &session_ctx as first parameter"
done

echo ""
echo "Done! Review the changes and manually fix .execute() calls to add &session_ctx parameter."
echo "Backup files created with .bak extension."
