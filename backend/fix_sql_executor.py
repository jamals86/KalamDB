#!/usr/bin/env python3
"""
Refactor SqlExecutor to use helper methods instead of direct field access.
Replaces self.field_name.as_ref().unwrap() with self.field_name() calls.
"""

import re
from pathlib import Path

# Field mappings: old pattern -> new method call
REPLACEMENTS = {
    # Pattern: self.field.as_ref().unwrap() -> self.field()
    r'self\.kalam_sql\.as_ref\(\)\.unwrap\(\)': 'self.kalam_sql()',
    r'self\.user_table_store\.as_ref\(\)\.unwrap\(\)': 'self.user_table_store()',
    r'self\.shared_table_store\.as_ref\(\)\.unwrap\(\)': 'self.shared_table_store()',
    r'self\.stream_table_store\.as_ref\(\)\.unwrap\(\)': 'self.stream_table_store()',
    r'self\.storage_backend\.as_ref\(\)\.unwrap\(\)': 'self.storage_backend()',
    r'self\.storage_registry\.as_ref\(\)\.unwrap\(\)': 'self.storage_registry()',
    r'self\.job_manager\.as_ref\(\)\.unwrap\(\)': 'self.job_manager()',
    r'self\.live_query_manager\.as_ref\(\)\.unwrap\(\)': 'self.live_query_manager()',
    r'self\.unified_cache\.as_ref\(\)\.unwrap\(\)': 'self.unified_cache()',
    r'self\.jobs_table_provider\.as_ref\(\)\.unwrap\(\)': 'self.jobs_table_provider()',
    r'self\.users_table_provider\.as_ref\(\)\.unwrap\(\)': 'self.users_table_provider()',
    r'self\.table_deletion_service\.as_ref\(\)\.unwrap\(\)': 'self.table_deletion_service',
    
    # Pattern: self.field.as_ref() -> Some(self.field())
    r'self\.kalam_sql\.as_ref\(\)': 'Some(&self.kalam_sql())',
    r'self\.storage_registry\.as_ref\(\)': 'Some(&self.storage_registry())',
    r'self\.job_manager\.as_ref\(\)': 'Some(&self.job_manager())',
    
    # Pattern: &self.session_factory -> &self.session_factory()
    r'&self\.session_factory(?!\(\))': '&self.session_factory()',
}

def fix_file(filepath):
    """Apply all replacements to the file."""
    with open(filepath, 'r') as f:
        content = f.read()
    
    original = content
    replacements_made = 0
    
    for pattern, replacement in REPLACEMENTS.items():
        new_content = re.sub(pattern, replacement, content)
        count = len(re.findall(pattern, content))
        if count > 0:
            print(f"  {pattern[:50]:50s} -> {replacement[:30]:30s} ({count:2d} times)")
            replacements_made += count
            content = new_content
    
    if content != original:
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"\n✅ Applied {replacements_made} replacements")
        return True
    else:
        print("\n⚠️  No changes needed")
        return False

if __name__ == '__main__':
    filepath = Path('crates/kalamdb-core/src/sql/executor.rs')
    print(f"Refactoring {filepath}...\n")
    fix_file(filepath)
