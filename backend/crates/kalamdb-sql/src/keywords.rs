//! Centralized SQL keyword enumerations.
//!
//! Provides strongly-typed representations for both ANSI SQL keywords and
//! KalamDB-specific extensions so parser modules can avoid duplicating
//! string literals.

use std::str::FromStr;

/// Standard SQL keywords supported by KalamDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SqlKeyword {
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Drop,
    Alter,
    Show,
    Describe,
    Flush,
    Subscribe,
    Kill,
}

impl SqlKeyword {
    pub fn as_str(self) -> &'static str {
        match self {
            SqlKeyword::Select => "SELECT",
            SqlKeyword::Insert => "INSERT",
            SqlKeyword::Update => "UPDATE",
            SqlKeyword::Delete => "DELETE",
            SqlKeyword::Create => "CREATE",
            SqlKeyword::Drop => "DROP",
            SqlKeyword::Alter => "ALTER",
            SqlKeyword::Show => "SHOW",
            SqlKeyword::Describe => "DESCRIBE",
            SqlKeyword::Flush => "FLUSH",
            SqlKeyword::Subscribe => "SUBSCRIBE",
            SqlKeyword::Kill => "KILL",
        }
    }
}

impl FromStr for SqlKeyword {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "SELECT" => Ok(SqlKeyword::Select),
            "INSERT" => Ok(SqlKeyword::Insert),
            "UPDATE" => Ok(SqlKeyword::Update),
            "DELETE" => Ok(SqlKeyword::Delete),
            "CREATE" => Ok(SqlKeyword::Create),
            "DROP" => Ok(SqlKeyword::Drop),
            "ALTER" => Ok(SqlKeyword::Alter),
            "SHOW" => Ok(SqlKeyword::Show),
            "DESCRIBE" | "DESC" => Ok(SqlKeyword::Describe),
            "FLUSH" => Ok(SqlKeyword::Flush),
            "SUBSCRIBE" => Ok(SqlKeyword::Subscribe),
            "KILL" => Ok(SqlKeyword::Kill),
            _ => Err(()),
        }
    }
}

/// KalamDB-specific keywords that extend standard SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KalamDbKeyword {
    Storage,
    Namespace,
    Flush,
    Job,
    Live,
    Table,
}

impl KalamDbKeyword {
    pub fn as_str(self) -> &'static str {
        match self {
            KalamDbKeyword::Storage => "STORAGE",
            KalamDbKeyword::Namespace => "NAMESPACE",
            KalamDbKeyword::Flush => "FLUSH",
            KalamDbKeyword::Job => "JOB",
            KalamDbKeyword::Live => "LIVE",
            KalamDbKeyword::Table => "TABLE",
        }
    }
}

impl FromStr for KalamDbKeyword {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "STORAGE" => Ok(KalamDbKeyword::Storage),
            "NAMESPACE" => Ok(KalamDbKeyword::Namespace),
            "FLUSH" => Ok(KalamDbKeyword::Flush),
            "JOB" => Ok(KalamDbKeyword::Job),
            "LIVE" => Ok(KalamDbKeyword::Live),
            "TABLE" => Ok(KalamDbKeyword::Table),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_keyword_from_str() {
        assert_eq!(SqlKeyword::from_str("select").unwrap(), SqlKeyword::Select);
        assert_eq!(SqlKeyword::from_str("DROP").unwrap(), SqlKeyword::Drop);
        assert!(SqlKeyword::from_str("UNKNOWN").is_err());
    }

    #[test]
    fn test_kalam_keyword_from_str() {
        assert_eq!(KalamDbKeyword::from_str("storage").unwrap(), KalamDbKeyword::Storage);
        assert!(KalamDbKeyword::from_str("foobar").is_err());
    }
}

