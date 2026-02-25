//! EXPORT USER DATA and SHOW EXPORT statement parsers
//!
//! Parses SQL statements:
//! - EXPORT USER DATA   — triggers an async export of all user tables
//! - SHOW EXPORT        — returns the latest export job status / download link

use crate::ddl::DdlResult;

/// EXPORT USER DATA statement
///
/// Initiates an asynchronous export of the calling user's data
/// across all namespaces and user tables. The export is packaged
/// as a ZIP archive and made available for download.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportUserDataStatement;

impl ExportUserDataStatement {
    /// Parse an `EXPORT USER DATA` statement from SQL
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql_upper = sql.trim().to_uppercase();

        if !sql_upper.starts_with("EXPORT USER DATA") {
            return Err("Expected EXPORT USER DATA statement".to_string());
        }

        // Nothing beyond the keywords is expected
        let remaining = sql.trim()["EXPORT USER DATA".len()..].trim();
        if !remaining.is_empty() {
            return Err(format!(
                "Unexpected tokens after EXPORT USER DATA: '{}'",
                remaining
            ));
        }

        Ok(Self)
    }
}

/// SHOW EXPORT statement
///
/// Returns the status of the most recent (or all) exports for the calling user,
/// including a download URL when the export is complete.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowExportStatement;

impl ShowExportStatement {
    /// Parse a `SHOW EXPORT` statement from SQL
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql_upper = sql.trim().to_uppercase();

        if !sql_upper.starts_with("SHOW EXPORT") {
            return Err("Expected SHOW EXPORT statement".to_string());
        }

        let remaining = sql.trim()["SHOW EXPORT".len()..].trim();
        if !remaining.is_empty() {
            return Err(format!(
                "Unexpected tokens after SHOW EXPORT: '{}'",
                remaining
            ));
        }

        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_export_user_data() {
        let stmt = ExportUserDataStatement::parse("EXPORT USER DATA").unwrap();
        assert_eq!(stmt, ExportUserDataStatement);
    }

    #[test]
    fn parse_export_user_data_case_insensitive() {
        let stmt = ExportUserDataStatement::parse("export user data").unwrap();
        assert_eq!(stmt, ExportUserDataStatement);
    }

    #[test]
    fn parse_export_user_data_extra_tokens_fails() {
        assert!(ExportUserDataStatement::parse("EXPORT USER DATA TO '/tmp'").is_err());
    }

    #[test]
    fn parse_show_export() {
        let stmt = ShowExportStatement::parse("SHOW EXPORT").unwrap();
        assert_eq!(stmt, ShowExportStatement);
    }

    #[test]
    fn parse_show_export_case_insensitive() {
        let stmt = ShowExportStatement::parse("show export").unwrap();
        assert_eq!(stmt, ShowExportStatement);
    }

    #[test]
    fn parse_show_export_extra_tokens_fails() {
        assert!(ShowExportStatement::parse("SHOW EXPORT ALL").is_err());
    }
}
