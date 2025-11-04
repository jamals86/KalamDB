#!/usr/bin/env python3
import re

# Read the file
with open('crates/kalamdb-core/src/sql/executor.rs', 'r') as f:
    content = f.read()

# 1. Fix multi-line method calls (field-like access)
methods = ['kalam_sql', 'job_manager', 'storage_backend', 'shared_table_store', 'user_table_store', 'stream_table_store']
for method in methods:
    pattern = rf'^(\s*)\.{method}$'
    replacement = rf'\1.{method}()'
    content = re.sub(pattern, replacement, content, flags=re.MULTILINE)

# 2. Fix single-line field access to method calls
all_methods = [
    'kalam_sql', 'user_table_store', 'shared_table_store', 'stream_table_store',
    'storage_backend', 'storage_registry', 'job_manager', 'live_query_manager',
    'unified_cache', 'jobs_table_provider', 'users_table_provider', 'session_factory'
]
for method in all_methods:
    # Match self.METHOD where METHOD is not followed by ( or )
    pattern = rf'\bself\.{method}(?!\()'
    replacement = rf'self.{method}()'
    content = re.sub(pattern, replacement, content)

# 3. Remove .as_ref().ok_or_else()? patterns for methods that return Arc<T> directly
store_methods = [
    'shared_table_store', 'stream_table_store', 'storage_backend',
    'storage_registry', 'unified_cache', 'user_table_store',
    'jobs_table_provider', 'users_table_provider', 'kalam_sql',
    'job_manager', 'live_query_manager'
]
for method in store_methods:
    # Pattern: self.METHOD().as_ref().ok_or_else(|| { error })?
    pattern = rf'self\.{method}\(\)\.as_ref\(\)[^;]*?\.ok_or_else\([^)]*\)\?'
    replacement = f'self.{method}()'
    content = re.sub(pattern, replacement, content, flags=re.DOTALL)

# 4. Fix unified_cache patterns: self.unified_cache().as_ref()...clone()
pattern = r'self\.unified_cache\(\)\.as_ref\(\)[^;]*?\.clone\(\)'
replacement = 'self.unified_cache()'
content = re.sub(pattern, replacement, content, flags=re.DOTALL)

# 5. Fix multi-line let assignments: let var = self\n.METHOD()\n.as_ref()\n.ok_or_else...
pattern = r'let (\w+) = self\s*\n\s*\.(\w+)\(\)\s*\n\s*\.as_ref\(\)\s*\n\s*\.ok_or_else[^;]*;'
replacement = r'let \1 = self.\2();'
content = re.sub(pattern, replacement, content, flags=re.MULTILINE)

# 6. Remove .is_some() checks (always true for Arc)
for method in ['user_table_store', 'shared_table_store', 'stream_table_store']:
    pattern = rf'if self\.{method}\(\)\.is_some\(\)'
    replacement = 'if true'
    content = re.sub(pattern, replacement, content)

# 7. Fix if let Some(var) = self.METHOD() patterns (METHOD returns Arc<T>, not Option)
# Need to handle this carefully - just unwrap the Arc reference
pattern = r'if let Some\((\w+)\) = &self\.(\w+)\(\) \{'
replacement = r'{ let \1 = &self.\2();'
content = re.sub(pattern, replacement, content)

# Write back
with open('crates/kalamdb-core/src/sql/executor.rs', 'w') as f:
    f.write(content)

print("Applied comprehensive executor fixes")
