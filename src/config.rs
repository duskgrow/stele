use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Config {
    pub server: ServerConfig,
    pub mcp: McpConfig,
    pub storage: StorageConfig,
    pub index: IndexConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct McpConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StorageConfig {
    pub fns: FnsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FnsConfig {
    pub base_url: String,
    pub api_token: String,
    pub default_vault: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct IndexConfig {
    pub db_path: String,
    pub embedding_dim: usize,
}

impl Config {
    /// Load configuration with priority: CLI arg → env var → default path.
    pub fn load(config_path: Option<&str>) -> anyhow::Result<Self> {
        let mut builder = config::Config::builder();

        builder = builder
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8080)?
            .set_default("mcp.endpoint", "/mcp")?
            .set_default("storage.fns.base_url", "http://localhost:9000")?
            .set_default("storage.fns.api_token", "")?
            .set_default("storage.fns.default_vault", "forge")?
            .set_default("index.embedding_dim", 1536)?;

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wikiops");
        let db_path = data_dir.join("brain.db").to_string_lossy().to_string();
        builder = builder.set_default("index.db_path", db_path)?;

        let file_path = if let Some(path) = config_path {
            PathBuf::from(path)
        } else if let Ok(env_path) = std::env::var("MY_BRAIN_CONFIG") {
            PathBuf::from(env_path)
        } else {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("wikiops")
                .join("config.toml")
        };

        if file_path.exists() {
            let file = config::File::new(
                file_path.to_str().unwrap_or("config.toml"),
                config::FileFormat::Toml,
            );
            builder = builder.add_source(file);
        }

        let settings = builder.build()?;
        let config: Config = settings.try_deserialize()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn create_temp_config(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_load_full_config_from_file() {
        let toml = r#"
[server]
host = "127.0.0.1"
port = 3000

[mcp]
endpoint = "/custom"
api_key = "test-key"

[storage.fns]
base_url = "http://fns.example.com"
api_token = "token123"
default_vault = "test"

[index]
db_path = "/tmp/test.db"
embedding_dim = 768
"#;
        let file = create_temp_config(toml);
        let config = Config::load(Some(file.path().to_str().unwrap())).unwrap();

        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.mcp.endpoint, "/custom");
        assert_eq!(config.mcp.api_key, Some("test-key".to_string()));
        assert_eq!(config.storage.fns.base_url, "http://fns.example.com");
        assert_eq!(config.storage.fns.api_token, "token123");
        assert_eq!(config.storage.fns.default_vault, "test");
        assert_eq!(config.index.db_path, "/tmp/test.db");
        assert_eq!(config.index.embedding_dim, 768);
    }

    #[test]
    fn test_load_partial_config_uses_defaults() {
        let toml = r#"
[server]
port = 9090
"#;
        let file = create_temp_config(toml);
        let config = Config::load(Some(file.path().to_str().unwrap())).unwrap();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9090);
        assert_eq!(config.mcp.endpoint, "/mcp");
        assert_eq!(config.mcp.api_key, None);
        assert_eq!(config.storage.fns.base_url, "http://localhost:9000");
        assert_eq!(config.storage.fns.default_vault, "forge");
        assert_eq!(config.index.embedding_dim, 1536);
        assert!(config.index.db_path.contains("brain.db"));
    }

    #[test]
    fn test_load_no_api_key_defaults_to_none() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 8080

[mcp]
endpoint = "/mcp"

[storage.fns]
base_url = "http://localhost:9000"
api_token = ""
default_vault = "forge"

[index]
db_path = "/tmp/brain.db"
embedding_dim = 1536
"#;
        let file = create_temp_config(toml);
        let config = Config::load(Some(file.path().to_str().unwrap())).unwrap();

        assert_eq!(config.mcp.api_key, None);
    }

    #[test]
    fn test_env_var_priority() {
        let _guard = ENV_LOCK.lock().unwrap();

        let toml = r#"
[server]
host = "from-env"
port = 1111
"#;
        let file = create_temp_config(toml);

        unsafe {
            std::env::set_var("MY_BRAIN_CONFIG", file.path().to_str().unwrap());
        }
        let config = Config::load(None).unwrap();
        unsafe {
            std::env::remove_var("MY_BRAIN_CONFIG");
        }

        assert_eq!(config.server.host, "from-env");
        assert_eq!(config.server.port, 1111);
    }

    #[test]
    fn test_cli_arg_overrides_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();

        let toml_env = r#"
[server]
host = "from-env"
"#;
        let toml_cli = r#"
[server]
host = "from-cli"
"#;
        let file_env = create_temp_config(toml_env);
        let file_cli = create_temp_config(toml_cli);

        unsafe {
            std::env::set_var("MY_BRAIN_CONFIG", file_env.path().to_str().unwrap());
        }
        let config = Config::load(Some(file_cli.path().to_str().unwrap())).unwrap();
        unsafe {
            std::env::remove_var("MY_BRAIN_CONFIG");
        }

        assert_eq!(config.server.host, "from-cli");
    }
}
