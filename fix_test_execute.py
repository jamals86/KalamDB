#!/usr/bin/env python3
"""
Fix Phase 5 test compatibility: Add &session_ctx parameter to all .execute() calls
"""

import re
import sys
from pathlib import Path

def fix_execute_calls(content: str) -> tuple[str, int]:
    """Add &session_ctx as first parameter to .execute() calls"""
    
    # Pattern 1: .execute("...", Some(&...))
    # Replace with: .execute(&session_ctx, "...", Some(&...))
    pattern1 = r'\.execute\(\s*"([^"]*)",\s*(Some\([^)]+\))\s*\)'
    replacement1 = r'.execute(&session_ctx, "\1", \2)'
    
    # Pattern 2: .execute(variable, Some(&...))
    # Replace with: .execute(&session_ctx, variable, Some(&...))
    pattern2 = r'\.execute\(\s*([a-zA-Z_][a-zA-Z0-9_]*),\s*(Some\([^)]+\))\s*\)'
    replacement2 = r'.execute(&session_ctx, \1, \2)'
    
    # Pattern 3: .execute(&variable, Some(&...))
    # Replace with: .execute(&session_ctx, &variable, Some(&...))
    pattern3 = r'\.execute\(\s*&([a-zA-Z_][a-zA-Z0-9_]*),\s*(Some\([^)]+\))\s*\)'
    replacement3 = r'.execute(&session_ctx, &\1, \2)'
    
    # Apply all replacements
    new_content = content
    count = 0
    
    # Pattern 1
    new_content, n1 = re.subn(pattern1, replacement1, new_content)
    count += n1
    
    # Pattern 2 (only if not already has session_ctx)
    pattern2_safe = r'\.execute\(\s*(?!&session_ctx)([a-zA-Z_][a-zA-Z0-9_]*),\s*(Some\([^)]+\))\s*\)'
    new_content, n2 = re.subn(pattern2_safe, replacement2, new_content)
    count += n2
    
    # Pattern 3 (only if not already has session_ctx)
    pattern3_safe = r'\.execute\(\s*(?!&session_ctx)&([a-zA-Z_][a-zA-Z0-9_]*),\s*(Some\([^)]+\))\s*\)'
    new_content, n3 = re.subn(pattern3_safe, replacement3, new_content)
    count += n3
    
    return new_content, count

def fix_test_signatures(content: str) -> tuple[str, int]:
    """Update test function signatures to include session_ctx"""
    
    # Pattern: let (executor, ..., kalam_sql) = setup...
    # Replace with: let (executor, ..., kalam_sql, session_ctx) = setup...
    pattern = r'let \((executor, [^,]+, kalam_sql)\) = setup'
    replacement = r'let (\1, session_ctx) = setup'
    
    new_content, count = re.subn(pattern, replacement, content)
    return new_content, count

def main():
    test_files = [
        "backend/tests/test_password_complexity.rs",
        "backend/tests/test_user_sql_commands.rs",
    ]
    
    for file_path in test_files:
        path = Path(file_path)
        if not path.exists():
            print(f"⚠️  Skipping {file_path} (not found)")
            continue
            
        print(f"📝 Fixing {file_path}...")
        
        content = path.read_text()
        
        # Fix test signatures
        content, sig_count = fix_test_signatures(content)
        
        # Fix execute() calls
        content, exec_count = fix_execute_calls(content)
        
        # Write back
        path.write_text(content)
        
        print(f"   ✓ Updated {sig_count} test signatures, {exec_count} .execute() calls")
    
    print("\n✅ Done!")

if __name__ == "__main__":
    main()
