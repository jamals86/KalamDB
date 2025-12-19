//! Command parser for SQL and backslash commands
//!
//! **Implements T087**: CommandParser for SQL + backslash commands
//!
//! Parses user input to distinguish between SQL statements and CLI meta-commands.

use crate::error::{CLIError, Result};

/// Parsed command
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// SQL statement
    Sql(String),

    /// Meta-commands (backslash commands)
    Quit,
    Help,
    Config,
    Flush,
    Health,
    Pause,
    Continue,
    ListTables,
    Describe(String),
    SetFormat(String),
    Subscribe(String),
    Unsubscribe,
    RefreshTables,
    ShowCredentials,
    UpdateCredentials {
        username: String,
        password: String,
    },
    DeleteCredentials,
    Info,
    /// Show system statistics (from system.stats)
    Stats,
    Unknown(String),
}

/// Command parser
pub struct CommandParser;

impl CommandParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Parse a command line
    pub fn parse(&self, line: &str) -> Result<Command> {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            return Err(CLIError::ParseError("Empty command".into()));
        }

        // Check for backslash commands
        if trimmed.starts_with('\\') {
            return self.parse_meta_command(trimmed);
        }

        // Otherwise, treat as SQL
        Ok(Command::Sql(trimmed.to_string()))
    }

    /// Parse meta-commands (backslash commands)
    fn parse_meta_command(&self, line: &str) -> Result<Command> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return Err(CLIError::ParseError("Invalid command".into()));
        }

        let command = parts[0];
        let args = parts.get(1..).unwrap_or(&[]);

        match command {
            "\\quit" | "\\q" => Ok(Command::Quit),
            "\\help" | "\\?" => Ok(Command::Help),
            "\\stats" | "\\metrics" => Ok(Command::Stats),
            "\\config" => Ok(Command::Config),
            "\\flush" => Ok(Command::Flush),
            "\\health" => Ok(Command::Health),
            "\\pause" => Ok(Command::Pause),
            "\\continue" => Ok(Command::Continue),
            "\\dt" | "\\tables" => Ok(Command::ListTables),
            "\\d" | "\\describe" => {
                if args.is_empty() {
                    Err(CLIError::ParseError(
                        "\\describe requires a table name".into(),
                    ))
                } else {
                    Ok(Command::Describe(args.join(" ")))
                }
            }
            "\\format" => {
                if args.is_empty() {
                    Err(CLIError::ParseError(
                        "\\format requires: table, json, or csv".into(),
                    ))
                } else {
                    Ok(Command::SetFormat(args[0].to_string()))
                }
            }
            "\\subscribe" | "\\watch" => {
                if args.is_empty() {
                    Err(CLIError::ParseError(
                        "\\subscribe requires a SQL query".into(),
                    ))
                } else {
                    Ok(Command::Subscribe(args.join(" ")))
                }
            }
            "\\unsubscribe" | "\\unwatch" => Ok(Command::Unsubscribe),
            "\\refresh-tables" | "\\refresh" => Ok(Command::RefreshTables),
            "\\show-credentials" | "\\credentials" => Ok(Command::ShowCredentials),
            "\\update-credentials" => {
                if args.len() < 2 {
                    Err(CLIError::ParseError(
                        "\\update-credentials requires username and password".into(),
                    ))
                } else {
                    Ok(Command::UpdateCredentials {
                        username: args[0].to_string(),
                        password: args[1].to_string(),
                    })
                }
            }
            "\\delete-credentials" => Ok(Command::DeleteCredentials),
            "\\info" | "\\session" => Ok(Command::Info),
            _ => Ok(Command::Unknown(command.to_string())),
        }
    }
}

impl Default for CommandParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sql() {
        let parser = CommandParser::new();
        let cmd = parser.parse("SELECT * FROM users").unwrap();
        assert_eq!(cmd, Command::Sql("SELECT * FROM users".to_string()));
    }

    #[test]
    fn test_parse_quit() {
        let parser = CommandParser::new();
        assert_eq!(parser.parse("\\quit").unwrap(), Command::Quit);
        assert_eq!(parser.parse("\\q").unwrap(), Command::Quit);
    }

    #[test]
    fn test_parse_help() {
        let parser = CommandParser::new();
        assert_eq!(parser.parse("\\help").unwrap(), Command::Help);
        assert_eq!(parser.parse("\\?").unwrap(), Command::Help);
    }

    #[test]
    fn test_parse_describe() {
        let parser = CommandParser::new();
        let cmd = parser.parse("\\describe users").unwrap();
        assert_eq!(cmd, Command::Describe("users".to_string()));
    }

    #[test]
    fn test_parse_unknown() {
        let parser = CommandParser::new();
        let cmd = parser.parse("\\unknown").unwrap();
        assert_eq!(cmd, Command::Unknown("\\unknown".to_string()));
    }

    #[test]
    fn test_parse_stats() {
        let parser = CommandParser::new();
        assert_eq!(parser.parse("\\stats").unwrap(), Command::Stats);
        assert_eq!(parser.parse("\\metrics").unwrap(), Command::Stats);
    }

    #[test]
    fn test_empty_command() {
        let parser = CommandParser::new();
        assert!(parser.parse("").is_err());
        assert!(parser.parse("   ").is_err());
    }
}
