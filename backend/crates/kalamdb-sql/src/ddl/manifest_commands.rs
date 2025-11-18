//! SHOW MANIFEST command parser (Phase 4, US6, T088)
//!
//! Parses `SHOW MANIFEST` command for inspecting manifest cache state.

/// SHOW MANIFEST statement
///
/// Returns all manifest cache entries with their metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowManifestStatement;

impl ShowManifestStatement {
    /// Parse SHOW MANIFEST command
    ///
    /// Syntax: `SHOW MANIFEST`
    ///
    /// # Arguments
    /// * `sql` - SQL string to parse
    ///
    /// # Returns
    /// * `Ok(ShowManifestStatement)` if syntax is valid
    /// * `Err(String)` if syntax is invalid
    pub fn parse(sql: &str) -> Result<Self, String> {
        // Normalize whitespace: split by whitespace and rejoin with single space
        let normalized = sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_uppercase();

        // Validate exact syntax
        if normalized != "SHOW MANIFEST" {
            return Err(format!(
                "Invalid SHOW MANIFEST syntax. Expected: SHOW MANIFEST, got: {}",
                sql
            ));
        }

        Ok(ShowManifestStatement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_show_manifest() {
        let sql = "SHOW MANIFEST";
        let result = ShowManifestStatement::parse(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_show_manifest_lowercase() {
        let sql = "show manifest";
        let result = ShowManifestStatement::parse(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_show_manifest_with_extra_whitespace() {
        let sql = "  SHOW   MANIFEST  ";
        let result = ShowManifestStatement::parse(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_invalid_syntax() {
        let sql = "SHOW MAN";
        let result = ShowManifestStatement::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid"));
    }
}
