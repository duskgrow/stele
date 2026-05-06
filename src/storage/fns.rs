use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use super::{BackendError, FileBackend, FileMeta, FileStat};

const MAX_RETRIES: u32 = 3;
const BACKOFF_DURATIONS: [Duration; 3] = [
    Duration::from_millis(100),
    Duration::from_millis(300),
    Duration::from_millis(900),
];

#[derive(Debug, Deserialize)]
struct FnsResponse {
    code: i32,
    data: Option<serde_json::Value>,
    msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NoteListItem {
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
}

pub struct FnsBackend {
    client: reqwest::Client,
    base_url: String,
    api_token: String,
    default_vault: String,
}

impl FnsBackend {
    pub fn new(base_url: String, api_token: String, default_vault: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            api_token,
            default_vault,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", &self.api_token)
    }

    async fn send_with_retry<F, Fut>(
        &self,
        build_request: F,
    ) -> Result<reqwest::Response, BackendError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<reqwest::RequestBuilder, BackendError>>,
    {
        let mut last_err = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let backoff = BACKOFF_DURATIONS[(attempt - 1) as usize];
                debug!("retry attempt {attempt}, sleeping {backoff:?}");
                tokio::time::sleep(backoff).await;
            }

            let request_builder = build_request().await?;
            match request_builder.send().await {
                Ok(resp) => return Ok(resp),
                Err(e) if e.is_timeout() || e.is_connect() => {
                    warn!("request failed (attempt {attempt}): {e}");
                    last_err = Some(e);
                }
                Err(e) => return Err(e.into()),
            }
        }

        Err(BackendError::Api {
            status: 0,
            message: format!("max retries exceeded: {}", last_err.unwrap()),
        })
    }

    fn map_response_status(resp: &reqwest::Response, path: &str) -> Result<(), BackendError> {
        match resp.status() {
            StatusCode::NOT_FOUND => Err(BackendError::NotFound(path.to_string())),
            StatusCode::CONFLICT => Err(BackendError::Conflict(path.to_string())),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(BackendError::Auth("unauthorized".into()))
            }
            s if s.is_success() => Ok(()),
            s => Err(BackendError::Api {
                status: s.as_u16(),
                message: format!("unexpected status {s}"),
            }),
        }
    }

    fn check_fns_code(json: &FnsResponse) -> Result<(), BackendError> {
        if json.code != 1 {
            return Err(BackendError::Api {
                status: 500,
                message: json
                    .msg
                    .clone()
                    .unwrap_or_else(|| format!("FNS error code: {}", json.code)),
            });
        }
        Ok(())
    }
}

impl From<reqwest::Error> for BackendError {
    fn from(e: reqwest::Error) -> Self {
        BackendError::Api {
            status: e.status().map(|s| s.as_u16()).unwrap_or(0),
            message: e.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl FileBackend for FnsBackend {
    async fn get(&self, path: &str) -> Result<String, BackendError> {
        let vault = self.default_vault.clone();
        let path_owned = path.to_string();
        let base_url = self.base_url.clone();
        let token = self.auth_header().to_string();

        let resp = self
            .send_with_retry(|| {
                let vault = vault.clone();
                let path_owned = path_owned.clone();
                let base_url = base_url.clone();
                let token = token.clone();
                async move {
                    Ok(self
                        .client
                        .get(format!("{base_url}/api/note"))
                        .query(&[("path", path_owned.as_str()), ("vault", vault.as_str())])
                        .header("Authorization", token.as_str()))
                }
            })
            .await?;

        Self::map_response_status(&resp, path)?;
        let json: FnsResponse = resp.json().await?;
        Self::check_fns_code(&json)?;

        json.data
            .and_then(|d| d.get("content").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| BackendError::Api {
                status: 500,
                message: "missing content in FNS response".into(),
            })
    }

    async fn put(&self, path: &str, content: &str) -> Result<(), BackendError> {
        let body = serde_json::json!({
            "path": path,
            "vault": self.default_vault,
            "content": content,
        });

        let resp = self
            .send_with_retry(|| {
                let body = body.clone();
                async move {
                    Ok(self
                        .client
                        .post(format!("{}/api/note", self.base_url))
                        .json(&body)
                        .header("Authorization", self.auth_header()))
                }
            })
            .await?;

        Self::map_response_status(&resp, path)?;
        let json: FnsResponse = resp.json().await?;
        Self::check_fns_code(&json)?;
        Ok(())
    }

    async fn append(&self, path: &str, content: &str) -> Result<(), BackendError> {
        let body = serde_json::json!({
            "path": path,
            "vault": self.default_vault,
            "content": content,
        });

        let resp = self
            .send_with_retry(|| {
                let body = body.clone();
                async move {
                    Ok(self
                        .client
                        .post(format!("{}/api/note/append", self.base_url))
                        .json(&body)
                        .header("Authorization", self.auth_header()))
                }
            })
            .await?;

        Self::map_response_status(&resp, path)?;
        let json: FnsResponse = resp.json().await?;
        Self::check_fns_code(&json)?;
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), BackendError> {
        let vault = self.default_vault.clone();
        let path_owned = path.to_string();
        let base_url = self.base_url.clone();
        let token = self.auth_header().to_string();

        let resp = self
            .send_with_retry(|| {
                let vault = vault.clone();
                let path_owned = path_owned.clone();
                let base_url = base_url.clone();
                let token = token.clone();
                async move {
                    Ok(self
                        .client
                        .delete(format!("{base_url}/api/note"))
                        .query(&[("path", path_owned.as_str()), ("vault", vault.as_str())])
                        .header("Authorization", token.as_str()))
                }
            })
            .await?;

        Self::map_response_status(&resp, path)?;
        let json: FnsResponse = resp.json().await?;
        Self::check_fns_code(&json)?;
        Ok(())
    }

    async fn list(&self, dir: &str) -> Result<Vec<FileMeta>, BackendError> {
        let vault = self.default_vault.clone();
        let dir_owned = dir.to_string();
        let base_url = self.base_url.clone();
        let token = self.auth_header().to_string();

        let resp = self
            .send_with_retry(|| {
                let vault = vault.clone();
                let dir_owned = dir_owned.clone();
                let base_url = base_url.clone();
                let token = token.clone();
                async move {
                    Ok(self
                        .client
                        .get(format!("{base_url}/api/folder/notes"))
                        .query(&[("path", dir_owned.as_str()), ("vault", vault.as_str())])
                        .header("Authorization", token.as_str()))
                }
            })
            .await?;

        Self::map_response_status(&resp, dir)?;
        let json: FnsResponse = resp.json().await?;
        Self::check_fns_code(&json)?;

        debug!(dir = dir, data = ?json.data, "FNS list raw response");
        let items: Vec<NoteListItem> = json
            .data
            .and_then(|d| serde_json::from_value(d).ok())
            .unwrap_or_default();

        Ok(items
            .into_iter()
            .map(|item| FileMeta {
                path: item.path,
                is_dir: false,
                size: item.size,
                modified: item.updated_at.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
            })
            .collect())
    }

    async fn exists(&self, path: &str) -> Result<bool, BackendError> {
        match self.get(path).await {
            Ok(_) => Ok(true),
            Err(BackendError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn stat(&self, path: &str) -> Result<FileStat, BackendError> {
        let content = self.get(path).await?;
        let size = content.len() as u64;
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        Ok(FileStat {
            size,
            modified: Utc::now(),
            content_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend(base_url: &str) -> FnsBackend {
        FnsBackend::new(
            base_url.to_string(),
            "test-token".to_string(),
            "test-vault".to_string(),
        )
    }

    #[test]
    fn test_fns_backend_construction() {
        let backend = make_backend("http://localhost:9000");
        assert_eq!(backend.base_url, "http://localhost:9000");
        assert_eq!(backend.api_token, "test-token");
        assert_eq!(backend.default_vault, "test-vault");
    }

    #[test]
    fn test_auth_header_has_bearer_prefix() {
        let backend = make_backend("http://localhost:9000");
        assert_eq!(backend.auth_header(), "Bearer test-token");
        assert!(backend.auth_header().starts_with("Bearer"));
    }

    #[test]
    fn test_backend_error_not_found_display() {
        let err = BackendError::NotFound("test.md".into());
        assert!(format!("{err}").contains("not found"));
    }

    #[test]
    fn test_check_fns_code_success() {
        let json = FnsResponse {
            code: 1,
            data: None,
            msg: None,
        };
        assert!(FnsBackend::check_fns_code(&json).is_ok());
    }

    #[test]
    fn test_check_fns_code_error() {
        let json = FnsResponse {
            code: 0,
            data: None,
            msg: Some("something went wrong".into()),
        };
        let err = FnsBackend::check_fns_code(&json).unwrap_err();
        match err {
            BackendError::Api { message, .. } => {
                assert!(message.contains("something went wrong"));
            }
            _ => panic!("expected Api error"),
        }
    }

    #[test]
    fn test_backoff_durations() {
        assert_eq!(BACKOFF_DURATIONS[0], Duration::from_millis(100));
        assert_eq!(BACKOFF_DURATIONS[1], Duration::from_millis(300));
        assert_eq!(BACKOFF_DURATIONS[2], Duration::from_millis(900));
    }
}
