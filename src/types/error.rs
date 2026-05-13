use thiserror::Error;

/// Unified error type for all stele operations.
#[derive(Error, Debug)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("fns error: {0}")]
    Fns(String),
    #[error("mcp error: {0}")]
    Mcp(String),
}

impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Error::NotFound(s) => Error::NotFound(s.clone()),
            Error::Conflict(s) => Error::Conflict(s.clone()),
            Error::Io(e) => Error::Io(std::io::Error::new(e.kind(), e.to_string())),
            Error::Storage(s) => Error::Storage(s.clone()),
            Error::Parse(s) => Error::Parse(s.clone()),
            Error::Config(s) => Error::Config(s.clone()),
            Error::Fns(s) => Error::Fns(s.clone()),
            Error::Mcp(s) => Error::Mcp(s.clone()),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Parse(err.to_string())
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Self {
        Error::Parse(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::Fns(err.to_string())
    }
}

/// Shorthand for `std::result::Result<T, stele::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_not_found() {
        let err = Error::NotFound("page/foo".to_string());
        assert_eq!(err.to_string(), "not found: page/foo");
    }

    #[test]
    fn error_display_conflict() {
        let err = Error::Conflict("page already exists".to_string());
        assert_eq!(err.to_string(), "conflict: page already exists");
    }

    #[test]
    fn error_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let err = Error::from(io_err);
        assert!(err.to_string().contains("io error"));
        assert!(err.to_string().contains("file gone"));
    }

    #[test]
    fn error_display_storage() {
        let err = Error::Storage("db locked".to_string());
        assert_eq!(err.to_string(), "storage error: db locked");
    }

    #[test]
    fn error_display_parse() {
        let err = Error::Parse("invalid yaml".to_string());
        assert_eq!(err.to_string(), "parse error: invalid yaml");
    }

    #[test]
    fn error_display_config() {
        let err = Error::Config("missing key".to_string());
        assert_eq!(err.to_string(), "config error: missing key");
    }

    #[test]
    fn error_display_fns() {
        let err = Error::Fns("request failed".to_string());
        assert_eq!(err.to_string(), "fns error: request failed");
    }

    #[test]
    fn error_display_mcp() {
        let err = Error::Mcp("tool not found".to_string());
        assert_eq!(err.to_string(), "mcp error: tool not found");
    }

    #[test]
    fn error_from_serde_json() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Parse(_)));
    }

    #[test]
    fn error_from_serde_yaml() {
        let yaml_err = serde_yaml::from_str::<i32>("not: a: number").unwrap_err();
        let err: Error = yaml_err.into();
        assert!(matches!(err, Error::Parse(_)));
    }

    #[test]
    fn result_type_alias() {
        let err: Result<i32> = Err(Error::NotFound("test".to_string()));
        assert!(err.is_err());
    }

    #[test]
    fn error_clone_all_variants() {
        let variants = vec![
            Error::NotFound("page".to_string()),
            Error::Conflict("exists".to_string()),
            Error::Io(std::io::Error::other("io")),
            Error::Storage("db".to_string()),
            Error::Parse("bad".to_string()),
            Error::Config("missing".to_string()),
            Error::Fns("fail".to_string()),
            Error::Mcp("mcp".to_string()),
        ];
        for err in variants {
            let cloned = err.clone();
            assert_eq!(err.to_string(), cloned.to_string());
        }
    }
}
