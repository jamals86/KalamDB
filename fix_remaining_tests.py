#!/usr/bin/env python3
"""Fix Arc<SessionContext> dereference issues in integration tests"""

import re
from pathlib import Path

def fix_session_context_refs(content: str) -> tuple[str, int]:
    """Fix &session_ctx to &*session_ctx where needed"""
    
    # Pattern: &session_ctx (not already dereferenced)
    # Replace with: &*session_ctx
    pattern = r'\.execute\(&session_ctx,'
    replacement = r'.execute(&*session_ctx,'
    
    new_content, count = re.subn(pattern, replacement, content)
    return new_content, count

def main():
    # Find all integration test files
    test_dir = Path("backend/tests/integration")
    test_files = list(test_dir.rglob("*.rs"))
    
    total_fixes = 0
    for file_path in test_files:
        content = file_path.read_text()
        new_content, count = fix_session_context_refs(content)
        
        if count > 0:
            file_path.write_text(new_content)
            print(f"✓ Fixed {count} references in {file_path.relative_to('backend/tests')}")
            total_fixes += count
    
    # Also fix standalone test files that might have this issue
    standalone_files = [
        "backend/tests/test_password_complexity.rs",
        "backend/tests/test_user_sql_commands.rs",
        "backend/tests/test_audit_logging.rs",
    ]
    
    for file_path_str in standalone_files:
        file_path = Path(file_path_str)
        if file_path.exists():
            content = file_path.read_text()
            new_content, count = fix_session_context_refs(content)
            
            if count > 0:
                file_path.write_text(new_content)
                print(f"✓ Fixed {count} references in {file_path.name}")
                total_fixes += count
    
    print(f"\n✅ Total fixes: {total_fixes}")

if __name__ == "__main__":
    main()
