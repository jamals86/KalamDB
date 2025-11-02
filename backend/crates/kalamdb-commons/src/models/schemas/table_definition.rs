//! Table definition - single source of truth for table schemas

use crate::models::schemas::{ColumnDefinition, SchemaVersion, TableOptions, TableType};
use crate::{NamespaceId, TableName};
use crate::models::types::{ArrowConversionError, ToArrowType};
use arrow_schema::{Field, Schema as ArrowSchema};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Complete definition of a table including schema, history, and options
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableDefinition {
    /// Namespace ID (e.g., "default", "user_123")
    pub namespace_id: NamespaceId,

    /// Table name (case-sensitive)
    pub table_name: TableName,

    /// Table type (USER, SHARED, STREAM, SYSTEM)
    pub table_type: TableType,

    /// Column definitions (ordered by ordinal_position)
    pub columns: Vec<ColumnDefinition>,

    /// Current schema version number
    pub schema_version: u32,

    /// Schema version history (embedded for atomic updates)
    /// Sorted by version (ascending)
    pub schema_history: Vec<SchemaVersion>,

    /// Type-safe table options based on table_type
    pub table_options: TableOptions,

    /// Table comment/description
    pub table_comment: Option<String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp
    pub updated_at: DateTime<Utc>,
}

impl TableDefinition {
    /// Create a new table definition
    ///
    /// # Example
    ///
    /// ```
    /// use kalamdb_commons::models::schemas::{TableDefinition, ColumnDefinition, TableType, TableOptions};
    /// use kalamdb_commons::models::types::KalamDataType;
    ///
    /// let columns = vec![
    ///     ColumnDefinition::primary_key("id", 1, KalamDataType::Uuid),
    ///     ColumnDefinition::simple("name", 2, KalamDataType::Text),
    ///     ColumnDefinition::simple("age", 3, KalamDataType::Int),
    /// ];
    ///
    /// let table = TableDefinition::new(
    ///     kalamdb_commons::NamespaceId::new("my_namespace"),
    ///     kalamdb_commons::TableName::new("users"),
    ///     TableType::User,
    ///     columns,
    ///     TableOptions::user(),
    ///     Some("User accounts table".into()),
    /// ).unwrap();
    ///
    /// assert_eq!(table.namespace_id, kalamdb_commons::NamespaceId::new("my_namespace"));
    /// assert_eq!(table.table_name, kalamdb_commons::TableName::new("users"));
    /// assert_eq!(table.columns.len(), 3);
    /// assert_eq!(table.schema_version, 1);
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        namespace_id: NamespaceId,
        table_name: TableName,
        table_type: TableType,
        columns: Vec<ColumnDefinition>,
        table_options: TableOptions,
        table_comment: Option<String>,
    ) -> Result<Self, String> {
        let columns_sorted = Self::validate_and_sort_columns(columns)?;
        let now = Utc::now();

        Ok(Self {
            namespace_id,
            table_name,
            table_type,
            columns: columns_sorted,
            schema_version: 1,
            schema_history: Vec::new(),
            table_options,
            table_comment,
            created_at: now,
            updated_at: now,
        })
    }

    /// Create a new table with default options for the table type
    pub fn new_with_defaults(
        namespace_id: NamespaceId,
        table_name: TableName,
        table_type: TableType,
        columns: Vec<ColumnDefinition>,
        table_comment: Option<String>,
    ) -> Result<Self, String> {
        let table_options = match table_type {
            TableType::User => TableOptions::user(),
            TableType::Shared => TableOptions::shared(),
            TableType::Stream => TableOptions::stream(86400), // Default 24h TTL
            TableType::System => TableOptions::system(),
        };

        Self::new(namespace_id, table_name, table_type, columns, table_options, table_comment)
    }

    /// Validate and sort columns by ordinal_position
    fn validate_and_sort_columns(
        mut columns: Vec<ColumnDefinition>,
    ) -> Result<Vec<ColumnDefinition>, String> {
        // Check for duplicate ordinal positions
        let mut positions = std::collections::HashSet::new();
        for col in &columns {
            if col.ordinal_position == 0 {
                return Err(format!(
                    "Column '{}' has invalid ordinal_position 0 (must be ≥ 1)",
                    col.column_name
                ));
            }
            if !positions.insert(col.ordinal_position) {
                return Err(format!(
                    "Duplicate ordinal_position {}",
                    col.ordinal_position
                ));
            }
        }

        // Sort by ordinal_position
        columns.sort_by_key(|col| col.ordinal_position);

        // Validate sequential positions starting from 1
        for (idx, col) in columns.iter().enumerate() {
            let expected = (idx + 1) as u32;
            if col.ordinal_position != expected {
                return Err(format!(
                    "Non-sequential ordinal_position: expected {}, got {}",
                    expected, col.ordinal_position
                ));
            }
        }

        Ok(columns)
    }

    /// Convert to Arrow schema (current version)
    pub fn to_arrow_schema(&self) -> Result<Arc<ArrowSchema>, ArrowConversionError> {
        let fields: Result<Vec<Field>, _> = self
            .columns
            .iter()
            .map(|col| {
                let arrow_type = col.data_type.to_arrow_type()?;
                Ok(Field::new(&col.column_name, arrow_type, col.is_nullable))
            })
            .collect();

        let schema = ArrowSchema::new(fields?);
        Ok(Arc::new(schema))
    }

    /// Get schema at a specific version
    pub fn get_schema_at_version(&self, version: u32) -> Option<&SchemaVersion> {
        self.schema_history.iter().find(|v| v.version == version)
    }

    /// Add a new schema version to history
    pub fn add_schema_version(
        &mut self,
        changes: impl Into<String>,
        arrow_schema_json: impl Into<String>,
    ) -> Result<(), String> {
        let new_version = self.schema_version + 1;
        let schema_version = SchemaVersion::new(new_version, changes, arrow_schema_json);

        self.schema_history.push(schema_version);
        self.schema_version = new_version;
        self.updated_at = Utc::now();

        Ok(())
    }

    /// Get the latest schema version from history
    pub fn get_latest_schema_version(&self) -> Option<&SchemaVersion> {
        self.schema_history.last()
    }

    /// Get a reference to the table options
    pub fn options(&self) -> &TableOptions {
        &self.table_options
    }

    /// Update table options (must match table type)
    pub fn set_options(&mut self, options: TableOptions) {
        self.table_options = options;
        self.updated_at = Utc::now();
    }

    /// Get fully qualified table name (namespace.table)
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.namespace_id.as_str(), self.table_name.as_str())
    }

    /// Add a column (for ALTER TABLE ADD COLUMN)
    pub fn add_column(&mut self, column: ColumnDefinition) -> Result<(), String> {
        // Validate ordinal_position is next available
        let max_ordinal = self
            .columns
            .iter()
            .map(|c| c.ordinal_position)
            .max()
            .unwrap_or(0);
        if column.ordinal_position != max_ordinal + 1 {
            return Err(format!(
                "New column must have ordinal_position {}, got {}",
                max_ordinal + 1,
                column.ordinal_position
            ));
        }

        self.columns.push(column);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Drop a column (for ALTER TABLE DROP COLUMN)
    /// Note: Does NOT renumber remaining columns' ordinal_position
    pub fn drop_column(&mut self, column_name: &str) -> Result<ColumnDefinition, String> {
        let position = self
            .columns
            .iter()
            .position(|c| c.column_name == column_name)
            .ok_or_else(|| format!("Column '{}' not found", column_name))?;

        let removed = self.columns.remove(position);
        self.updated_at = Utc::now();
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::types::KalamDataType;
    use crate::{NamespaceId, TableName};

    fn sample_columns() -> Vec<ColumnDefinition> {
        vec![
            ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt),
            ColumnDefinition::simple("name", 2, KalamDataType::Text),
            ColumnDefinition::simple("age", 3, KalamDataType::Int),
        ]
    }

    #[test]
    fn test_new_table_definition() {
        let table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            sample_columns(),
            TableOptions::user(),
            Some("User table".to_string()),
        )
        .unwrap();

        assert_eq!(table.namespace_id, NamespaceId::new("default"));
        assert_eq!(table.table_name, TableName::new("users"));
        assert_eq!(table.table_type, TableType::User);
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.schema_version, 1);
        assert_eq!(table.qualified_name(), "default.users");
    }

    #[test]
    fn test_new_with_defaults() {
        let table = TableDefinition::new_with_defaults(
            NamespaceId::new("default"),
            TableName::new("events"),
            TableType::Stream,
            sample_columns(),
            None,
        )
        .unwrap();

        assert_eq!(table.table_type, TableType::Stream);
        // Should have default stream options with 24h TTL
        if let TableOptions::Stream(opts) = &table.table_options {
            assert_eq!(opts.ttl_seconds, 86400);
        } else {
            panic!("Expected Stream options");
        }
    }

    #[test]
    fn test_column_ordering() {
        // Create columns out of order
        let columns = vec![
            ColumnDefinition::simple("name", 2, KalamDataType::Text),
            ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt),
            ColumnDefinition::simple("age", 3, KalamDataType::Int),
        ];

        let table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        )
        .unwrap();

        // Verify columns are sorted by ordinal_position
        assert_eq!(table.columns[0].column_name, "id");
        assert_eq!(table.columns[1].column_name, "name");
        assert_eq!(table.columns[2].column_name, "age");
    }

    #[test]
    fn test_duplicate_ordinal_position() {
        let columns = vec![
            ColumnDefinition::simple("col1", 1, KalamDataType::Int),
            ColumnDefinition::simple("col2", 1, KalamDataType::Int),
        ];

        let result = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("test"),
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate ordinal_position"));
    }

    #[test]
    fn test_non_sequential_ordinal_position() {
        let columns = vec![
            ColumnDefinition::simple("col1", 1, KalamDataType::Int),
            ColumnDefinition::simple("col2", 3, KalamDataType::Int), // Skips 2
        ];

        let result = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("test"),
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Non-sequential"));
    }

    #[test]
    fn test_to_arrow_schema() {
        let table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            sample_columns(),
            TableOptions::user(),
            None,
        )
        .unwrap();

        let arrow_schema = table.to_arrow_schema().unwrap();
        assert_eq!(arrow_schema.fields().len(), 3);
        assert_eq!(arrow_schema.field(0).name(), "id");
        assert_eq!(arrow_schema.field(1).name(), "name");
        assert_eq!(arrow_schema.field(2).name(), "age");
    }

    #[test]
    fn test_add_schema_version() {
        let mut table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            sample_columns(),
            TableOptions::user(),
            None,
        )
        .unwrap();

        table
            .add_schema_version("Added email column", "{}")
            .unwrap();
        assert_eq!(table.schema_version, 2);
        assert_eq!(table.schema_history.len(), 1);
        assert_eq!(table.schema_history[0].version, 2);
    }

    #[test]
    fn test_get_schema_at_version() {
        let mut table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            sample_columns(),
            TableOptions::user(),
            None,
        )
        .unwrap();

        table.add_schema_version("V2", "{}").unwrap();
        table.add_schema_version("V3", "{}").unwrap();

        assert!(table.get_schema_at_version(2).is_some());
        assert!(table.get_schema_at_version(3).is_some());
        assert!(table.get_schema_at_version(1).is_none()); // Version 1 has no history entry
        assert!(table.get_schema_at_version(4).is_none());
    }

    #[test]
    fn test_add_column() {
        let mut table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            sample_columns(),
            TableOptions::user(),
            None,
        )
        .unwrap();

        let new_col = ColumnDefinition::simple("email", 4, KalamDataType::Text);
        table.add_column(new_col).unwrap();

        assert_eq!(table.columns.len(), 4);
        assert_eq!(table.columns[3].column_name, "email");
    }

    #[test]
    fn test_drop_column() {
        let mut table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            sample_columns(),
            TableOptions::user(),
            None,
        )
        .unwrap();

        let dropped = table.drop_column("name").unwrap();
        assert_eq!(dropped.column_name, "name");
        assert_eq!(table.columns.len(), 2);

        // Verify ordinal_position preserved
        assert_eq!(table.columns[0].ordinal_position, 1); // id
        assert_eq!(table.columns[1].ordinal_position, 3); // age (not renumbered)
    }

    #[test]
    fn test_table_options_type_safety() {
        // Test USER table options
        let user_table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("users"),
            TableType::User,
            sample_columns(),
            TableOptions::user(),
            None,
        )
        .unwrap();

        assert!(matches!(user_table.options(), TableOptions::User(_)));

        // Test STREAM table options
        let stream_table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("events"),
            TableType::Stream,
            sample_columns(),
            TableOptions::stream(3600),
            None,
        )
        .unwrap();

        if let TableOptions::Stream(opts) = stream_table.options() {
            assert_eq!(opts.ttl_seconds, 3600);
        } else {
            panic!("Expected Stream options");
        }

        // Test SHARED table options
        let shared_table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("categories"),
            TableType::Shared,
            sample_columns(),
            TableOptions::shared(),
            None,
        )
        .unwrap();

        if let TableOptions::Shared(opts) = shared_table.options() {
            assert!(opts.enable_cache);
            assert_eq!(opts.access_level, "public");
        } else {
            panic!("Expected Shared options");
        }
    }

    #[test]
    fn test_set_options() {
        let mut table = TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new("events"),
            TableType::Stream,
            sample_columns(),
            TableOptions::stream(3600),
            None,
        )
        .unwrap();

        // Update to custom stream options
        let custom_opts = TableOptions::Stream(crate::models::schemas::StreamTableOptions {
            ttl_seconds: 1800,
            eviction_strategy: "size_based".to_string(),
            max_stream_size_bytes: 1_000_000_000,
            enable_compaction: false,
            watermark_delay_seconds: 120,
            compression: "lz4".to_string(),
        });

        table.set_options(custom_opts);

        if let TableOptions::Stream(opts) = table.options() {
            assert_eq!(opts.ttl_seconds, 1800);
            assert_eq!(opts.eviction_strategy, "size_based");
        } else {
            panic!("Expected Stream options");
        }
    }
}
