#!/usr/bin/env python3
import re

# Read the file
with open('crates/kalamdb-core/src/sql/executor.rs', 'r') as f:
    content = f.read()

# Pattern to match: self.METHOD().as_ref().ok_or_else(|| { ... })?
# We want to replace this with just: self.METHOD()
methods = [
    'shared_table_store', 'stream_table_store', 'storage_backend',
    'storage_registry', 'unified_cache', 'user_table_store',
    'jobs_table_provider', 'users_table_provider'
]

for method in methods:
    # Pattern: self.METHOD().as_ref().ok_or_else(|| { error message })?
    # Replace with: self.METHOD()
    pattern = rf'self\.{method}\(\)\.as_ref\(\)\s*\.ok_or_else\(\|\|\s*\{{[^}}]*\}}\s*\)\?'
    replacement = f'self.{method}()'
    content = re.sub(pattern, replacement, content, flags=re.MULTILINE | re.DOTALL)
    
    # Also handle multi-line patterns like:
    # let cache = self.METHOD().as_ref()
    #     .ok_or_else(|| { error })?;
    pattern2 = rf'self\.{method}\(\)\.as_ref\(\)\s*\n\s*\.ok_or_else\([^)]*\)\?'
    replacement2 = f'self.{method}()'
    content = re.sub(pattern2, replacement2, content, flags=re.MULTILINE)

# Write back
with open('crates/kalamdb-core/src/sql/executor.rs', 'w') as f:
    f.write(content)

print("Fixed method patterns")
