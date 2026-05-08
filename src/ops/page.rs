use serde_json::json;
use tracing::warn;

use crate::fns::FnsClient;
use crate::index::IndexEngine;
use crate::parser::{page as page_parser, wikilink};
use crate::types::{Error, Result};

pub async fn handle_page_get(
    fns: &FnsClient,
    index: &IndexEngine,
    slug: &str,
) -> Result<serde_json::Value> {
    let content = fns
        .get_note(slug)
        .await
        .map_err(|e| Error::Fns(format!("failed to fetch page '{slug}': {e}")))?;
    let page = page_parser::parse_page(&content, slug)?;
    let metadata = index.get_page(slug).await?;

    Ok(json!({
        "slug": slug,
        "content": content,
        "frontmatter": page.frontmatter,
        "metadata": metadata,
    }))
}

pub async fn handle_page_put(
    fns: &FnsClient,
    index: &IndexEngine,
    slug: &str,
    content: &str,
    etag: Option<&str>,
) -> Result<serde_json::Value> {
    if let Some(expected_etag) = etag {
        if let Some(existing) = index.get_page(slug).await? {
            if existing.content_hash != expected_etag {
                return Err(Error::Conflict(format!(
                    "etag mismatch: expected {}, got {}",
                    expected_etag, existing.content_hash
                )));
            }
        }
    }

    let page = page_parser::parse_page(content, slug)?;

    fns.put_note(slug, content)
        .await
        .map_err(|e| Error::Fns(format!("failed to save page '{slug}': {e}")))?;

    let index_result = index.index_page(&page).await;
    if let Err(ref e) = index_result {
        warn!("index_page failed for {}: {}", slug, e);
    }

    let links = wikilink::extract_links_for_page(&page.compiled_truth, slug);

    let links_result = index.update_links(slug, &links).await;
    if let Err(ref e) = links_result {
        warn!("update_links failed for {}: {}", slug, e);
    }

    Ok(json!({
        "slug": slug,
        "indexed": index_result.is_ok(),
        "links_count": links.len(),
    }))
}

pub async fn handle_page_delete(
    fns: &FnsClient,
    index: &IndexEngine,
    slug: &str,
) -> Result<serde_json::Value> {
    fns.delete_note(slug)
        .await
        .map_err(|e| Error::Fns(format!("failed to delete page '{slug}': {e}")))?;
    index.remove_page(slug).await?;

    Ok(json!({
        "slug": slug,
        "deleted": true,
    }))
}

pub async fn handle_page_list(
    fns: &FnsClient,
    dir: Option<&str>,
) -> Result<serde_json::Value> {
    let files = fns.list_notes(dir.unwrap_or(".")).await?;
    let count = files.len();

    Ok(json!({
        "files": files,
        "count": count,
    }))
}

pub async fn handle_page_append(
    fns: &FnsClient,
    index: &IndexEngine,
    slug: &str,
    content: &str,
) -> Result<serde_json::Value> {
    fns.append_note(slug, content)
        .await
        .map_err(|e| Error::Fns(format!("failed to append to page '{slug}': {e}")))?;

    // Re-read the full note to update the local index
    let full_content = fns
        .get_note(slug)
        .await
        .map_err(|e| Error::Fns(format!("failed to re-read page '{slug}' after append: {e}")))?;

    let page = page_parser::parse_page(&full_content, slug)?;

    let index_result = index.index_page(&page).await;
    if let Err(ref e) = index_result {
        warn!("index_page failed for {}: {}", slug, e);
    }

    let links = wikilink::extract_links_for_page(&page.compiled_truth, slug);

    let links_result = index.update_links(slug, &links).await;
    if let Err(ref e) = links_result {
        warn!("update_links failed for {}: {}", slug, e);
    }

    Ok(json!({
        "slug": slug,
        "appended": true,
        "indexed": index_result.is_ok(),
        "links_count": links.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_markdown(slug: &str) -> String {
        format!(
            "---\ntitle: Test Page\npage_type: Concept\ntags:\n  - rust\nrelated: []\nsources: []\nstatus: Budding\n---\nThis is content for [[{slug}]].\n---\n- 2024-01-01: First entry\n"
        )
    }

    fn markdown_with_links() -> &'static str {
        "---\ntitle: Link Page\npage_type: Concept\ntags: []\nrelated: []\nsources: []\nstatus: Budding\n---\nSee [[page-a]] and [[cites::page-b|Reference]].\n"
    }

    async fn setup_fns_and_index() -> (FnsClient, IndexEngine, MockServer) {
        let server = MockServer::start().await;
        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );
        let index = IndexEngine::new(":memory:").await.expect("in-memory index");
        (fns, index, server)
    }

    #[tokio::test]
    async fn test_page_put_get_roundtrip() {
        let (fns, index, server) = setup_fns_and_index().await;
        let slug = "test-page";
        let content = sample_markdown(slug);

        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", slug))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": content, "path": slug, "fileLinks": {}, "version": 1 }
            })))
            .expect(1)
            .mount(&server)
            .await;

        handle_page_put(&fns, &index, slug, &content, None)
            .await
            .expect("put should succeed");

        let result = handle_page_get(&fns, &index, slug)
            .await
            .expect("get should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert_eq!(result["content"].as_str().unwrap(), content);
        assert!(result["frontmatter"].is_object());
        assert!(result["metadata"].is_object());
    }

    #[tokio::test]
    async fn test_page_put_indexes() {
        let (fns, index, server) = setup_fns_and_index().await;
        let slug = "indexed-page";
        let content = sample_markdown(slug);

        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_put(&fns, &index, slug, &content, None)
            .await
            .expect("put should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert_eq!(result["indexed"].as_bool().unwrap(), true);

        let page = index.get_page(slug).await.expect("get_page should succeed");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.slug, slug);
        assert_eq!(page.frontmatter.title, "Test Page");
    }

    #[tokio::test]
    async fn test_page_put_extracts_links() {
        let (fns, index, server) = setup_fns_and_index().await;
        let slug = "link-page";
        let content = markdown_with_links();

        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_put(&fns, &index, slug, content, None)
            .await
            .expect("put should succeed");

        assert_eq!(result["links_count"].as_u64().unwrap(), 2);

        let outgoing = graph::get_outlinks(index.pool(), slug).await.expect("get_outlinks should succeed");
        assert_eq!(outgoing.len(), 2);

        let target_slugs: Vec<&str> = outgoing.iter().map(|l| l.target_slug.as_str()).collect();
        assert!(target_slugs.contains(&"page-a"));
        assert!(target_slugs.contains(&"page-b"));
    }

    #[tokio::test]
    async fn test_page_delete() {
        let (fns, index, server) = setup_fns_and_index().await;
        let slug = "delete-page";
        let content = sample_markdown(slug);

        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        handle_page_put(&fns, &index, slug, &content, None)
            .await
            .expect("put should succeed");

        assert!(index.get_page(slug).await.unwrap().is_some());

        let result = handle_page_delete(&fns, &index, slug)
            .await
            .expect("delete should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert_eq!(result["deleted"].as_bool().unwrap(), true);
        assert!(index.get_page(slug).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_etag_conflict() {
        let (fns, index, server) = setup_fns_and_index().await;
        let slug = "etag-page";
        let content = sample_markdown(slug);

        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let first = handle_page_put(&fns, &index, slug, &content, None)
            .await
            .expect("first put should succeed");
        assert_eq!(first["indexed"].as_bool().unwrap(), true);

        let wrong_etag = "wrong-etag-hash";
        let result = handle_page_put(&fns, &index, slug, &content, Some(wrong_etag)).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Conflict(msg) => {
                assert!(msg.contains("etag mismatch"));
            }
            other => panic!("expected Conflict error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_fns_failure() {
        let (fns, index, server) = setup_fns_and_index().await;
        let slug = "fails-page";
        let content = sample_markdown(slug);

        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .expect(4)
            .mount(&server)
            .await;

        let result = handle_page_put(&fns, &index, slug, &content, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Fns(_) => {}
            other => panic!("expected Fns error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_page_list() {
        let (fns, _index, server) = setup_fns_and_index().await;

        let response_data = json!({
            "list": [
                {"path": "page-a.md"},
                {"path": "page-b.md"}
            ],
            "pager": { "totalRows": 2 }
        });
        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": response_data
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_list(&fns, None)
            .await
            .expect("list should succeed");

        let files = result["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].as_str().unwrap(), "page-a.md");
        assert_eq!(files[1].as_str().unwrap(), "page-b.md");
        assert_eq!(result["count"].as_u64().unwrap(), 2);
    }
}
