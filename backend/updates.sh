# 1. Update all dependency lockfiles
cargo update

# 2. Check outdated dependencies across workspace
cargo outdated --workspace --depth 1

# 3. Check for duplicate versions
cargo tree -d

# 4. Upgrade compatible ones
cargo upgrade --workspace

# 5. Audit for vulnerabilities
cargo audit