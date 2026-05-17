use crate::config::FnsConfig;
use crate::types::{Error, Result};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// Percent-encode a string for use as a URL query parameter value.
fn encode_query_value(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push('%');
                result.push_str(&format!("{:02X}", byte));
            }
        }
    }
    result
}

#[derive(Debug, Deserialize)]
struct FnsResponse {
    code: i32,
    #[allow(dead_code)]
    status: Option<bool>,
    data: Option<Value>,
    message: Option<String>,
}

/// HTTP client for communicating with an FNS vault server.
pub struct FnsClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
    vault: String,
}

impl FnsClient {
    /// Create a new client with explicit connection parameters.
    pub fn new(base_url: String, token: String, vault: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client should build with default settings");
        Self {
            client,
            base_url,
            token,
            vault,
        }
    }

    /// Create a client from application configuration.
    pub fn from_config(config: &FnsConfig) -> Self {
        Self::new(
            config.base_url.clone(),
            config.token.clone(),
            config.vault.clone(),
        )
    }

    /// Fetch the raw markdown content of a note.
    pub async fn get_note(&self, path: &str) -> Result<String> {
        let url = format!(
            "{}/api/note?vault={}&path={}",
            self.base_url,
            encode_query_value(&self.vault),
            encode_query_value(path),
        );
        let resp = self
            .execute_request(|| async {
                self.client
                    .get(&url)
                    .header("token", &self.token)
                    .send()
                    .await
            })
            .await?;
        self.parse_response(resp).await.and_then(|v| {
            v.get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| Error::Parse("expected object with content field".to_string()))
        })
    }

    /// Upload or overwrite a note.
    pub async fn put_note(&self, path: &str, content: &str) -> Result<()> {
        let url = format!("{}/api/note", self.base_url);
        let resp = self
            .execute_request(|| async {
                self.client
                    .post(&url)
                    .header("token", &self.token)
                    .json(&serde_json::json!({
                        "vault": self.vault,
                        "path": path,
                        "content": content,
                    }))
                    .send()
                    .await
            })
            .await?;
        self.parse_response(resp).await.map(|_| ())
    }

    /// Append content to the end of an existing note.
    pub async fn append_note(&self, path: &str, content: &str) -> Result<()> {
        let url = format!("{}/api/note/append", self.base_url);
        let resp = self
            .execute_request(|| async {
                self.client
                    .post(&url)
                    .header("token", &self.token)
                    .json(&serde_json::json!({
                        "vault": self.vault,
                        "path": path,
                        "content": content,
                    }))
                    .send()
                    .await
            })
            .await?;
        self.parse_response(resp).await.map(|_| ())
    }

    /// Delete a note by path.
    pub async fn delete_note(&self, path: &str) -> Result<()> {
        let url = format!(
            "{}/api/note?vault={}&path={}",
            self.base_url,
            encode_query_value(&self.vault),
            encode_query_value(path),
        );
        let resp = self
            .execute_request(|| async {
                self.client
                    .delete(&url)
                    .header("token", &self.token)
                    .send()
                    .await
            })
            .await?;
        self.parse_response(resp).await.map(|_| ())
    }

    /// List all note files in a directory.
    pub async fn list_notes(&self, dir: &str) -> Result<Vec<String>> {
        let mut all_notes = Vec::new();
        let mut page = 1;
        let page_size = 100;

        loop {
            let url = format!(
                "{}/api/folder/notes?vault={}&path={}&page={}&pageSize={}",
                self.base_url,
                encode_query_value(&self.vault),
                encode_query_value(dir),
                page,
                page_size
            );
            let resp = self
                .execute_request(|| async {
                    self.client
                        .get(&url)
                        .header("token", &self.token)
                        .send()
                        .await
                })
                .await?;
            let data = self.parse_response(resp).await?;

            // Extract list array — can be null for empty folders
            let list = data
                .get("list")
                .and_then(|l| l.as_array())
                .cloned()
                .unwrap_or_default();

            let total_rows = data
                .get("pager")
                .and_then(|p| p.get("totalRows"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0);

            for item in list {
                if let Some(path) = item.get("path").and_then(|p| p.as_str()) {
                    all_notes.push(path.to_string());
                }
            }

            if (page as u64 * page_size as u64) >= total_rows || total_rows == 0 {
                break;
            }
            page += 1;
        }

        Ok(all_notes)
    }

    /// List all subdirectories in a directory.
    pub async fn list_folders(&self, dir: &str) -> Result<Vec<String>> {
        let url = format!(
            "{}/api/folders?vault={}&path={}",
            self.base_url,
            encode_query_value(&self.vault),
            encode_query_value(dir),
        );
        let resp = self
            .execute_request(|| async {
                self.client
                    .get(&url)
                    .header("token", &self.token)
                    .send()
                    .await
            })
            .await?;
        let data = self.parse_response(resp).await?;

        let folders = data.as_array().cloned().unwrap_or_default();
        let mut result = Vec::new();
        for folder in folders {
            if let Some(path) = folder.get("path").and_then(|p| p.as_str()) {
                result.push(path.to_string());
            }
        }
        Ok(result)
    }

    async fn execute_request<F, Fut>(&self, request_fn: F) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<reqwest::Response, reqwest::Error>>,
    {
        let delays = [100, 300, 900];

        for delay in delays {
            match request_fn().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_server_error() {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        continue;
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }

        match request_fn().await {
            Ok(resp) => Ok(resp),
            Err(e) => Err(e.into()),
        }
    }

    async fn parse_response(&self, resp: reqwest::Response) -> Result<Value> {
        let status = resp.status();

        if status.is_server_error() {
            let body = resp.text().await.map_err(|e| Error::Fns(e.to_string()))?;
            return Err(Error::Fns(format!("server error {}: {}", status, body)));
        }

        let body = resp.text().await.map_err(|e| Error::Fns(e.to_string()))?;

        if status == StatusCode::NOT_FOUND {
            return Err(Error::NotFound(body));
        }
        if status == StatusCode::CONFLICT {
            return Err(Error::Conflict(body));
        }
        if status.is_client_error() {
            return Err(Error::Fns(body));
        }

        let fns_resp: FnsResponse = serde_json::from_str(&body)?;
        if fns_resp.code == 1 {
            Ok(fns_resp.data.unwrap_or(Value::Null))
        } else if fns_resp.code == 430 {
            Err(Error::NotFound(
                fns_resp
                    .message
                    .unwrap_or_else(|| "Note does not exist".to_string()),
            ))
        } else {
            Err(Error::Fns(fns_resp.message.unwrap_or_default()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_get_note_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", "notes/hello.md"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "path": "notes/hello.md",
                    "content": "note content",
                    "fileLinks": {},
                    "version": 1
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/hello.md").await;
        assert_eq!(result.unwrap(), "note content");
    }

    #[tokio::test]
    async fn test_get_note_sends_token_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(header("token", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "path": "notes/hello.md",
                    "content": "note content",
                    "fileLinks": {},
                    "version": 1
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/hello.md").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_put_note_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.put_note("notes/hello.md", "new content").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_retry_on_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(200)))
            .expect(4)
            .mount(&server)
            .await;

        let client = FnsClient {
            client: reqwest::Client::builder()
                .timeout(Duration::from_millis(50))
                .build()
                .unwrap(),
            base_url: server.uri(),
            token: "test-token".to_string(),
            vault: "test-vault".to_string(),
        };
        let result = client.get_note("notes/hello.md").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Fns(_)));
    }

    #[tokio::test]
    async fn test_retry_on_5xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .expect(4)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/hello.md").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Fns(_)));
    }

    #[tokio::test]
    async fn test_no_retry_on_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/hello.md").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Fns(_)));
    }

    #[tokio::test]
    async fn test_404_returns_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/missing.md").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound(_)));
    }

    #[tokio::test]
    async fn test_invalid_response_format() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/hello.md").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Parse(_)));
    }

    #[tokio::test]
    async fn test_unreachable_returns_error() {
        let client = FnsClient::new(
            "http://127.0.0.1:65432".to_string(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/hello.md").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Fns(_)));
    }

    #[tokio::test]
    async fn test_from_config() {
        let config = crate::config::FnsConfig {
            base_url: "http://localhost:3000".to_string(),
            token: "my-token".to_string(),
            vault: "my-vault".to_string(),
        };
        let client = FnsClient::from_config(&config);
        assert_eq!(client.base_url, "http://localhost:3000");
        assert_eq!(client.token, "my-token");
        assert_eq!(client.vault, "my-vault");
    }

    #[tokio::test]
    async fn test_append_note_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/note/append"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.append_note("notes/hello.md", "extra content").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_folders_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", "notes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": [
                    {"path": "wiki", "pathHash": "abc123"},
                    {"path": "skills", "pathHash": "def456"}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.list_folders("notes").await;
        assert_eq!(result.unwrap(), vec!["wiki", "skills"]);
    }

    #[tokio::test]
    async fn test_list_folders_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", "empty"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.list_folders("empty").await;
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_notes_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", "notes"))
            .and(query_param("page", "1"))
            .and(query_param("pageSize", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [
                        {"path": "notes/hello.md", "title": "Hello"},
                        {"path": "notes/world.md", "title": "World"}
                    ],
                    "pager": {"page": 1, "pageSize": 100, "totalRows": 2}
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.list_notes("notes").await;
        assert_eq!(result.unwrap(), vec!["notes/hello.md", "notes/world.md"]);
    }

    #[tokio::test]
    async fn test_list_notes_null_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", "empty"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": null,
                    "pager": {"page": 1, "pageSize": 100, "totalRows": 0}
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.list_notes("empty").await;
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_notes_pagination() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", "notes"))
            .and(query_param("page", "1"))
            .and(query_param("pageSize", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [
                        {"path": "notes/a.md"}, {"path": "notes/b.md"}, {"path": "notes/c.md"},
                        {"path": "notes/d.md"}, {"path": "notes/e.md"}, {"path": "notes/f.md"},
                        {"path": "notes/g.md"}, {"path": "notes/h.md"}, {"path": "notes/i.md"},
                        {"path": "notes/j.md"}
                    ],
                    "pager": {"page": 1, "pageSize": 100, "totalRows": 150}
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", "notes"))
            .and(query_param("page", "2"))
            .and(query_param("pageSize", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [
                        {"path": "notes/k.md"}, {"path": "notes/l.md"}, {"path": "notes/m.md"},
                        {"path": "notes/n.md"}, {"path": "notes/o.md"}
                    ],
                    "pager": {"page": 2, "pageSize": 100, "totalRows": 150}
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.list_notes("notes").await.unwrap();
        assert_eq!(result.len(), 15);
        assert_eq!(result[0], "notes/a.md");
        assert_eq!(result[14], "notes/o.md");
    }

    #[tokio::test]
    async fn test_conflict_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(409).set_body_string("conflict"))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.put_note("notes/hello.md", "content").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Conflict(_)));
    }

    #[tokio::test]
    async fn test_fns_error_with_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 430,
                "status": false,
                "message": "Note does not exist",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let result = client.get_note("notes/hello.md").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Note does not exist"));
    }
}
