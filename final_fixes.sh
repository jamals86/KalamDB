#!/bin/bash
# Final fixes for remaining 33 errors

cd "$(dirname "$0")"

echo "Applying final 33 fixes..."

# 1. Add missing imports
echo "1. Adding missing imports..."
perl -i -pe 'if (/^use crate::stores::system_table::UserTableStoreExt;/ && !$added) { $_ .= "use crate::stores::system_table::UserTableStoreExt;\n"; $added=1; }' \
  backend/crates/kalamdb-core/src/tables/user_tables/user_table_update.rs || \
  perl -i -pe 's/(use crate::tables::user_tables::user_table_store)/use crate::stores::system_table::UserTableStoreExt;\n$1/' \
  backend/crates/kalamdb-core/src/tables/user_tables/user_table_update.rs

perl -i -pe 's/(use crate::tables::user_tables::user_table_store)/use crate::stores::system_table::UserTableStoreExt;\n$1/' \
  backend/crates/kalamdb-core/src/tables/user_tables/user_table_delete.rs

# 2. Fix system_table.rs key.as_ref() - change to proper variable
echo "2. Fixing system_table.rs Sized errors..."
perl -i -0777 -pe 's/for \(key, row\) in all_rows \{[^}]*String::from_utf8_lossy\(key\.as_ref\(\)\)[^}]*\}/for (key, row) in all_rows {\n            drop(row); \/\/ unused\n            let key_str = String::from_utf8_lossy(key.as_ref()).to_string();\n            let key_id = UserTableRowId::new(UserId::new(""), \&key_str);\n            results.push(key_str);\n        }/g' \
  backend/crates/kalamdb-core/src/stores/system_table.rs

# 3. Fix change_detector.rs ambiguous calls
echo "3. Fixing change_detector.rs..."
perl -i -pe 's/self\.store\s*\.get\(/UserTableStoreExt::get(self.store.as_ref(),/' \
  backend/crates/kalamdb-core/src/live_query/change_detector.rs

perl -i -pe 's/self\.store\s*\.delete\(/UserTableStoreExt::delete(self.store.as_ref(),/' \
  backend/crates/kalamdb-core/src/live_query/change_detector.rs

#4. Fix user_table_insert.rs .as_ref()
echo "4. Fixing user_table_insert.rs..."
perl -i -pe 's/SharedTableStoreExt::put\(\s*&self\.store,/SharedTableStoreExt::put(self.store.as_ref(),/' \
  backend/crates/kalamdb-core/src/tables/user_tables/user_table_insert.rs

# 5. Fix table_deletion_service.rs ambiguous drop_table
echo "5. Fixing table_deletion_service.rs..."
perl -i -0777 -pe 's/(TableType::User\s*=>\s*self\.user_table_store\s*\.as_ref[^.]*\.)drop_table/$1 drop_table/' \
  backend/crates/kalamdb-core/src/services/table_deletion_service.rs
perl -i -pe 's/(TableType::User.*\.as_ref[^.]*\.)drop_table/UserTableStoreExt::drop_table(self.user_table_store.as_ref().ok_or_else(|| KalamDbError::InvalidOperation("Store not configured".to_string()))?,/' \
  backend/crates/kalamdb-core/src/services/table_deletion_service.rs

# 6. Fix user_table_update/delete ambiguous put/delete
echo "6. Fixing ambiguous trait calls..."
perl -i -0777 -pe 's/self\.store\s*\.put\(/UserTableStoreExt::put(self.store.as_ref(),/gs' \
  backend/crates/kalamdb-core/src/tables/user_tables/user_table_update.rs

perl -i -0777 -pe 's/self\.store\s*\.delete\(/UserTableStoreExt::delete(self.store.as_ref(),/gs' \
  backend/crates/kalamdb-core/src/tables/user_tables/user_table_delete.rs

# 7. Fix executor.rs SharedTableStoreExt::scan .as_ref()
echo "7. Fixing executor.rs Arc issue..."
perl -i -pe 's/self\.shared_table_store\.as_ref\(\)\.ok_or_else/self.shared_table_store.as_ref().ok_or_else/' \
  backend/crates/kalamdb-core/src/sql/executor.rs

echo ""
echo "Done! Running cargo check..."
cargo check 2>&1 | tail -5
