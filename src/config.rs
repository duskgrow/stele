use config::{Config as ConfigBuilder, ConfigError, File};
use serde::{Deserialize, Serialize};

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub server: ServerConfig,
    pub fns: FnsConfig,
    pub index: IndexConfig,
}

/// HTTP server settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// FNS (vault) server settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FnsConfig {
    pub base_url: String,
    pub token: String,
    pub vault: String,
}

/// Local index (SQLite) settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexConfig {
    pub db_path: String,
}

impl Default for Config {
    fn default() -> Self {
        let db_path = dirs::data_dir()
            .map(|d| d.join("stele/index.db").to_string_lossy().to_string())
            .unwrap_or_else(|| "./stele.db".to_string());

        Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            fns: FnsConfig {
                base_url: "http://localhost:3000".to_string(),
                token: String::new(),
                vault: "default".to_string(),
            },
            index: IndexConfig { db_path },
        }
    }
}

impl Config {
    /// Load configuration with the following priority (high → low):
    /// 1. Environment variables (`STELE_*`)
    /// 2. Config file (TOML)
    /// 3. Hard-coded defaults
    ///
    /// Config file is resolved via:
    /// 1. `STELE_CONFIG` environment variable
    /// 2. `~/.config/stele/config.toml`
    /// 3. `./config.toml`
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::resolve_config_path();
        let mut cfg = Self::load_from_path(&config_path)?;
        Self::apply_env_overrides(&mut cfg);
        Ok(cfg)
    }

    fn resolve_config_path() -> String {
        std::env::var("STELE_CONFIG").unwrap_or_else(|_| {
            dirs::config_dir()
                .map(|d| d.join("stele/config.toml").to_string_lossy().to_string())
                .unwrap_or_else(|| "./config.toml".to_string())
        })
    }

    fn load_from_path(path: &str) -> Result<Self, ConfigError> {
        let default_db_path = dirs::data_dir()
            .map(|d| d.join("stele/index.db").to_string_lossy().to_string())
            .unwrap_or_else(|| "./stele.db".to_string());

        ConfigBuilder::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 3000)?
            .set_default("fns.base_url", "http://localhost:3000")?
            .set_default("fns.token", "")?
            .set_default("fns.vault", "default")?
            .set_default("index.db_path", default_db_path)?
            .add_source(File::with_name(path).required(false))
            .build()?
            .try_deserialize()
    }

    fn apply_env_overrides(cfg: &mut Config) {
        if let Ok(val) = std::env::var("STELE_SERVER_HOST") {
            cfg.server.host = val;
        }
        if let Ok(val) = std::env::var("STELE_SERVER_PORT") {
            if let Ok(port) = val.parse::<u16>() {
                cfg.server.port = port;
            }
        }
        if let Ok(val) = std::env::var("STELE_FNS_BASE_URL") {
            cfg.fns.base_url = val;
        }
        if let Ok(val) = std::env::var("STELE_FNS_TOKEN") {
            cfg.fns.token = val;
        }
        if let Ok(val) = std::env::var("STELE_FNS_VAULT") {
            cfg.fns.vault = val;
        }
        if let Ok(val) = std::env::var("STELE_INDEX_DB_PATH") {
            cfg.index.db_path = val;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    /// Serialize access to environment variables during tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn default_db_path() -> String {
        dirs::data_dir()
            .map(|d| d.join("stele/index.db").to_string_lossy().to_string())
            .unwrap_or_else(|| "./stele.db".to_string())
    }

    #[test]
    fn test_defaults() {
        let cfg = Config::load_from_path("/nonexistent/path/config.toml").unwrap();
        assert_eq!(cfg.server.host, "127.0.0.1");
        assert_eq!(cfg.server.port, 3000);
        assert_eq!(cfg.fns.base_url, "http://localhost:3000");
        assert_eq!(cfg.fns.token, "");
        assert_eq!(cfg.fns.vault, "default");
        assert_eq!(cfg.index.db_path, default_db_path());
    }

    #[test]
    fn test_full_config() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        write!(
            file,
            r#"
[server]
host = "0.0.0.0"
port = 8080

[fns]
base_url = "http://example.com"
token = "secret"
vault = "custom"

[index]
db_path = "/custom/path.db"
"#
        )
        .unwrap();

        let cfg = Config::load_from_path(file.path().to_str().unwrap()).unwrap();
        assert_eq!(cfg.server.host, "0.0.0.0");
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.fns.base_url, "http://example.com");
        assert_eq!(cfg.fns.token, "secret");
        assert_eq!(cfg.fns.vault, "custom");
        assert_eq!(cfg.index.db_path, "/custom/path.db");
    }

    #[test]
    fn test_partial_config() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        write!(
            file,
            r#"
[server]
host = "0.0.0.0"

[fns]
vault = "custom"
"#
        )
        .unwrap();

        let cfg = Config::load_from_path(file.path().to_str().unwrap()).unwrap();
        assert_eq!(cfg.server.host, "0.0.0.0");
        assert_eq!(cfg.server.port, 3000);
        assert_eq!(cfg.fns.base_url, "http://localhost:3000");
        assert_eq!(cfg.fns.token, "");
        assert_eq!(cfg.fns.vault, "custom");
        assert_eq!(cfg.index.db_path, default_db_path());
    }

    #[test]
    fn test_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();

        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        write!(
            file,
            r#"
[server]
host = "0.0.0.0"
port = 8080

[fns]
base_url = "http://example.com"
token = "secret"
vault = "custom"

[index]
db_path = "/custom/path.db"
"#
        )
        .unwrap();

        unsafe {
            std::env::set_var("STELE_CONFIG", file.path().to_str().unwrap());
            std::env::set_var("STELE_SERVER_HOST", "1.2.3.4");
            std::env::set_var("STELE_SERVER_PORT", "9090");
            std::env::set_var("STELE_FNS_BASE_URL", "http://env.override");
            std::env::set_var("STELE_FNS_TOKEN", "env_token");
            std::env::set_var("STELE_FNS_VAULT", "env_vault");
            std::env::set_var("STELE_INDEX_DB_PATH", "/env/path.db");
        }

        let cfg = Config::load().unwrap();

        assert_eq!(cfg.server.host, "1.2.3.4");
        assert_eq!(cfg.server.port, 9090);
        assert_eq!(cfg.fns.base_url, "http://env.override");
        assert_eq!(cfg.fns.token, "env_token");
        assert_eq!(cfg.fns.vault, "env_vault");
        assert_eq!(cfg.index.db_path, "/env/path.db");

        unsafe {
            std::env::remove_var("STELE_CONFIG");
            std::env::remove_var("STELE_SERVER_HOST");
            std::env::remove_var("STELE_SERVER_PORT");
            std::env::remove_var("STELE_FNS_BASE_URL");
            std::env::remove_var("STELE_FNS_TOKEN");
            std::env::remove_var("STELE_FNS_VAULT");
            std::env::remove_var("STELE_INDEX_DB_PATH");
        }
    }

    #[test]
    fn test_missing_file() {
        let cfg = Config::load_from_path("/definitely/does/not/exist.toml").unwrap();
        assert_eq!(cfg.server.host, "127.0.0.1");
        assert_eq!(cfg.server.port, 3000);
        assert_eq!(cfg.fns.base_url, "http://localhost:3000");
        assert_eq!(cfg.fns.token, "");
        assert_eq!(cfg.fns.vault, "default");
        assert_eq!(cfg.index.db_path, default_db_path());
    }

    #[test]
    fn test_config_path_priority() {
        let _guard = ENV_LOCK.lock().unwrap();

        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        write!(
            file,
            r#"
[server]
host = "from.stele.config"
port = 7777
"#
        )
        .unwrap();

        let path = file.path().to_str().unwrap().to_string();
        unsafe {
            std::env::set_var("STELE_CONFIG", &path);
        }

        let cfg = Config::load().unwrap();
        assert_eq!(cfg.server.host, "from.stele.config");
        assert_eq!(cfg.server.port, 7777);

        unsafe {
            std::env::remove_var("STELE_CONFIG");
        }
    }
}
