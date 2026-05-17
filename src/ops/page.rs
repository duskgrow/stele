use chrono::Utc;
use serde_json::{Value, json};
use tracing::warn;

use crate::fns::FnsClient;
use crate::index::IndexEngine;
use crate::ops::is_hidden_path;
use crate::parser::frontmatter;
use crate::parser::page as page_parser;
use crate::parser::wikilink;
use crate::types::{Error, Frontmatter, Page, Result, TimelineAppendInput, TimelineEntry};

pub async fn handle_page_get(
    fns: &FnsClient,
    _index: &IndexEngine,
    slug: &str,
) -> Result<serde_json::Value> {
    let slug = page_parser::normalize_slug(slug)?;
    let fns_path = page_parser::to_fns_path(&slug);

    let content = match fns.get_note(&fns_path).await {
        Ok(content) => content,
        Err(Error::NotFound(msg)) => return Err(Error::NotFound(msg)),
        Err(e) => return Err(Error::Fns(format!("failed to fetch page '{slug}': {e}"))),
    };

    let page = page_parser::parse_page(&content, &slug)?;

    Ok(json!({
        "slug": slug,
        "frontmatter": page.frontmatter,
        "body": page.compiled_truth,
        "timeline": page.timeline,
        "content_hash": page.content_hash,
    }))
}

pub async fn handle_page_put(
    fns: &FnsClient,
    index: &IndexEngine,
    slug: &str,
    body: &str,
    frontmatter_updates: Option<&Value>,
    timeline_append: TimelineAppendInput,
    etag: Option<&str>,
) -> Result<serde_json::Value> {
    let slug = page_parser::normalize_slug(slug)?;
    let fns_path = page_parser::to_fns_path(&slug);

    if let Some(expected_etag) = etag {
        if let Some(existing) = index.get_page(&slug).await? {
            if existing.content_hash != expected_etag {
                return Err(Error::Conflict(format!(
                    "etag mismatch: expected {}, got {}",
                    expected_etag, existing.content_hash
                )));
            }
        }
    }

    let today = Utc::now().format("%Y-%m-%d").to_string();
    let new_entry = TimelineEntry {
        date: today,
        source_url: None,
        content: timeline_append.content,
        agent: timeline_append.agent,
    };

    let page = match fns.get_note(&fns_path).await {
        Ok(existing_content) => {
            let mut existing = page_parser::parse_page(&existing_content, &slug)?;
            let updates = frontmatter_updates
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            existing.frontmatter = frontmatter::merge_frontmatter(&existing.frontmatter, &updates)?;
            existing.compiled_truth = body.to_string();
            existing.timeline.push(new_entry);
            existing
        }
        Err(Error::NotFound(_)) => {
            let updates = frontmatter_updates.ok_or_else(|| {
                Error::Parse("frontmatter is required when creating a new page".to_string())
            })?;
            let title = updates
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    Error::Parse("title is required in frontmatter for new pages".to_string())
                })?;
            if title.is_empty() {
                return Err(Error::Parse(
                    "title must not be empty for new pages".to_string(),
                ));
            }
            let fm = frontmatter::merge_frontmatter(&Frontmatter::default(), updates)?;
            Page {
                slug: slug.clone(),
                frontmatter: fm,
                compiled_truth: body.to_string(),
                timeline: vec![new_entry],
                content_hash: String::new(),
                raw_content: String::new(),
            }
        }
        Err(e) => return Err(Error::Fns(format!("failed to fetch page '{slug}': {e}"))),
    };

    let serialized = page_parser::serialize_page(&page)?;

    fns.put_note(&fns_path, &serialized)
        .await
        .map_err(|e| Error::Fns(format!("failed to save page '{slug}': {e}")))?;

    let reparsed = page_parser::parse_page(&serialized, &slug)?;

    let index_result = index.index_page(&reparsed).await;
    if let Err(ref e) = index_result {
        warn!("index_page failed for {}: {}", slug, e);
    }

    let links = wikilink::extract_links_for_page(&reparsed.compiled_truth, &slug);
    let links_result = index.update_links(&slug, &links).await;
    if let Err(ref e) = links_result {
        warn!("update_links failed for {}: {}", slug, e);
    }

    Ok(json!({
        "slug": slug,
        "content_hash": reparsed.content_hash,
        "indexed": index_result.is_ok(),
        "links_count": links.len(),
        "timeline_count": reparsed.timeline.len(),
    }))
}

pub async fn handle_page_delete(
    fns: &FnsClient,
    index: &IndexEngine,
    slug: &str,
) -> Result<serde_json::Value> {
    let slug = page_parser::normalize_slug(slug)?;
    let fns_path = page_parser::to_fns_path(&slug);

    fns.delete_note(&fns_path)
        .await
        .map_err(|e| Error::Fns(format!("failed to delete page '{slug}': {e}")))?;
    index.remove_page(&slug).await?;

    Ok(json!({
        "slug": slug,
        "deleted": true,
    }))
}

pub async fn handle_page_list(fns: &FnsClient, dir: Option<&str>) -> Result<serde_json::Value> {
    let dir = dir.unwrap_or(".");
    let (files, folders) = tokio::join!(fns.list_notes(dir), fns.list_folders(dir),);

    let files: Vec<String> = files?.into_iter().filter(|p| !is_hidden_path(p)).collect();
    let folders: Vec<String> = folders?
        .into_iter()
        .filter(|p| !is_hidden_path(p))
        .collect();
    let count = files.len() + folders.len();

    Ok(json!({
        "files": files,
        "folders": folders,
        "count": count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph;
    use crate::test_utils::setup_test_fns_and_index;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    fn sample_frontmatter_json(_slug: &str) -> serde_json::Value {
        serde_json::json!({
            "title": "Test Page",
            "page_type": "Concept",
            "tags": ["rust"],
            "related": [],
            "sources": [],
            "status": "Budding"
        })
    }

    fn sample_body(slug: &str) -> String {
        format!("This is content for [[{slug}]].\n")
    }

    fn sample_timeline_entry() -> TimelineAppendInput {
        TimelineAppendInput {
            content: "Initial entry".into(),
            agent: None,
        }
    }

    #[tokio::test]
    async fn test_page_put_get_roundtrip() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "test-page";
        let body = sample_body(slug);

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

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

        let fm_json = sample_frontmatter_json(slug);
        let expected_md = {
            let fm = frontmatter::merge_frontmatter(&Frontmatter::default(), &fm_json).unwrap();
            let page = Page {
                slug: slug.to_string(),
                frontmatter: fm,
                compiled_truth: body.clone(),
                timeline: vec![TimelineEntry {
                    date: Utc::now().format("%Y-%m-%d").to_string(),
                    source_url: None,
                    content: "Initial entry".to_string(),
                    agent: None,
                }],
                content_hash: String::new(),
                raw_content: String::new(),
            };
            page_parser::serialize_page(&page).unwrap()
        };
        let get_content = expected_md.clone();

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", format!("{slug}.md")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": get_content, "path": slug, "fileLinks": {}, "version": 1 }
            })))
            .expect(1)
            .mount(&server)
            .await;

        handle_page_put(
            &fns,
            &index,
            slug,
            &body,
            Some(&fm_json),
            sample_timeline_entry(),
            None,
        )
        .await
        .expect("put should succeed");

        let result = handle_page_get(&fns, &index, slug)
            .await
            .expect("get should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert!(result["frontmatter"].is_object());
        assert_eq!(
            result["frontmatter"]["title"].as_str().unwrap(),
            "Test Page"
        );
        assert!(result["body"].is_string());
        assert!(result["timeline"].is_array());
        assert_eq!(result["timeline"].as_array().unwrap().len(), 1);
        assert!(result["content_hash"].is_string());
    }

    #[tokio::test]
    async fn test_page_get_structured_output() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "structured-page";
        let md = "---\ntitle: Structured Test\npage_type: Concept\ntags:\n  - rust\n  - test\nsources:\n  - https://example.com\n---\nThis is the compiled truth.\n---\n- 2024-01-01 [agent-a]: First entry\n- 2024-06-15 [https://source.com]: Second entry\n";

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", format!("{slug}.md")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": md, "path": slug, "fileLinks": {}, "version": 1 }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_get(&fns, &index, slug)
            .await
            .expect("get should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);

        let frontmatter = result["frontmatter"].as_object().unwrap();
        assert_eq!(frontmatter["title"].as_str().unwrap(), "Structured Test");
        assert_eq!(frontmatter["page_type"].as_str().unwrap(), "Concept");
        let tags = frontmatter["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].as_str().unwrap(), "rust");
        assert_eq!(tags[1].as_str().unwrap(), "test");

        assert_eq!(
            result["body"].as_str().unwrap(),
            "This is the compiled truth."
        );

        let timeline = result["timeline"].as_array().unwrap();
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0]["date"].as_str().unwrap(), "2024-01-01");
        assert_eq!(timeline[0]["content"].as_str().unwrap(), "First entry");
        assert_eq!(timeline[0]["agent"].as_str().unwrap(), "agent-a");
        assert_eq!(timeline[0]["source_url"], Value::Null);
        assert_eq!(timeline[1]["date"].as_str().unwrap(), "2024-06-15");
        assert_eq!(timeline[1]["content"].as_str().unwrap(), "Second entry");
        assert_eq!(
            timeline[1]["source_url"].as_str().unwrap(),
            "https://source.com"
        );
        assert_eq!(timeline[1]["agent"], Value::Null);

        assert!(result["content_hash"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_page_get_empty_timeline() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "no-timeline-page";
        let md = "---\ntitle: No Timeline\npage_type: Entity\ntags: []\nsources: []\n---\nJust the body.\n";

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", format!("{slug}.md")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": md, "path": slug, "fileLinks": {}, "version": 1 }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_get(&fns, &index, slug)
            .await
            .expect("get should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert_eq!(result["body"].as_str().unwrap(), "Just the body.");
        let timeline = result["timeline"].as_array().unwrap();
        assert!(timeline.is_empty());
    }

    #[tokio::test]
    async fn test_page_get_not_found() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "missing-page";

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .and(query_param("vault", "test-vault"))
            .and(query_param("path", format!("{slug}.md")))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_get(&fns, &index, slug).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::NotFound(msg) => {
                assert!(
                    msg.contains("not found") || msg.contains("Note does not exist"),
                    "expected not found message, got: {msg}"
                );
            }
            other => panic!("expected NotFound error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_page_put_indexes() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "indexed-page";
        let body = sample_body(slug);
        let fm_json = sample_frontmatter_json(slug);

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

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

        let result = handle_page_put(
            &fns,
            &index,
            slug,
            &body,
            Some(&fm_json),
            sample_timeline_entry(),
            None,
        )
        .await
        .expect("put should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert!(result["indexed"].as_bool().unwrap());

        let page = index.get_page(slug).await.expect("get_page should succeed");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.slug, slug);
        assert_eq!(page.frontmatter.title, "Test Page");
    }

    #[tokio::test]
    async fn test_page_put_extracts_links() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "link-page";
        let body = "See [[page-a]] and [[cites::page-b|Reference]].\n";
        let fm_json = serde_json::json!({
            "title": "Link Page",
            "page_type": "Concept",
            "tags": [],
            "related": [],
            "sources": [],
            "status": "Budding"
        });

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

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

        let result = handle_page_put(
            &fns,
            &index,
            slug,
            body,
            Some(&fm_json),
            sample_timeline_entry(),
            None,
        )
        .await
        .expect("put should succeed");

        assert_eq!(result["links_count"].as_u64().unwrap(), 2);

        let outgoing = graph::get_outlinks(index.pool(), slug, None)
            .await
            .expect("get_outlinks should succeed");
        assert_eq!(outgoing.len(), 2);

        let target_slugs: Vec<&str> = outgoing.iter().map(|l| l.target_slug.as_str()).collect();
        assert!(target_slugs.contains(&"page-a"));
        assert!(target_slugs.contains(&"page-b"));
    }

    #[tokio::test]
    async fn test_page_delete() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "delete-page";
        let body = sample_body(slug);
        let fm_json = sample_frontmatter_json(slug);

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

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

        handle_page_put(
            &fns,
            &index,
            slug,
            &body,
            Some(&fm_json),
            sample_timeline_entry(),
            None,
        )
        .await
        .expect("put should succeed");

        assert!(index.get_page(slug).await.unwrap().is_some());

        let result = handle_page_delete(&fns, &index, slug)
            .await
            .expect("delete should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert!(result["deleted"].as_bool().unwrap());
        assert!(index.get_page(slug).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_etag_conflict() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "etag-page";
        let body = sample_body(slug);
        let fm_json = sample_frontmatter_json(slug);

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

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

        let first = handle_page_put(
            &fns,
            &index,
            slug,
            &body,
            Some(&fm_json),
            sample_timeline_entry(),
            None,
        )
        .await
        .expect("first put should succeed");
        assert!(first["indexed"].as_bool().unwrap());

        let wrong_etag = "wrong-etag-hash";
        let result = handle_page_put(
            &fns,
            &index,
            slug,
            &body,
            Some(&fm_json),
            sample_timeline_entry(),
            Some(wrong_etag),
        )
        .await;

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
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "fails-page";
        let body = sample_body(slug);
        let fm_json = sample_frontmatter_json(slug);

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .expect(4)
            .mount(&server)
            .await;

        let result = handle_page_put(
            &fns,
            &index,
            slug,
            &body,
            Some(&fm_json),
            sample_timeline_entry(),
            None,
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Fns(_) => {}
            other => panic!("expected Fns error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_page_list_root() {
        let (fns, _index, server) = setup_test_fns_and_index().await;

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [{"path": "note.md"}],
                    "pager": { "totalRows": 1 }
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": [
                    {"path": "wiki", "pathHash": "abc"},
                    {"path": "test", "pathHash": "def"}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_list(&fns, None)
            .await
            .expect("list should succeed");

        let files = result["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].as_str().unwrap(), "note.md");

        let folders = result["folders"].as_array().unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].as_str().unwrap(), "wiki");
        assert_eq!(folders[1].as_str().unwrap(), "test");

        assert_eq!(result["count"].as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_page_list_subfolder() {
        let (fns, _index, server) = setup_test_fns_and_index().await;

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [],
                    "pager": { "totalRows": 0 }
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": [
                    {"path": "bugs", "pathHash": "abc"}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_list(&fns, Some("wiki"))
            .await
            .expect("list should succeed");

        let files = result["files"].as_array().unwrap();
        assert!(files.is_empty());

        let folders = result["folders"].as_array().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].as_str().unwrap(), "bugs");

        assert_eq!(result["count"].as_u64().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_page_list_leaf() {
        let (fns, _index, server) = setup_test_fns_and_index().await;

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [],
                    "pager": { "totalRows": 0 }
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_list(&fns, Some("leaf"))
            .await
            .expect("list should succeed");

        assert!(result["files"].as_array().unwrap().is_empty());
        assert!(result["folders"].as_array().unwrap().is_empty());
        assert_eq!(result["count"].as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_page_list_excludes_dot_prefixed_entries() {
        let (fns, _index, server) = setup_test_fns_and_index().await;

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": {
                    "list": [
                        {"path": ".secret.md"},
                        {"path": "readme.md"},
                        {"path": ".env.md"}
                    ],
                    "pager": { "totalRows": 3 }
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": [
                    {"path": ".archive", "pathHash": "abc"},
                    {"path": "wiki", "pathHash": "def"},
                    {"path": ".hidden", "pathHash": "ghi"}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_list(&fns, None)
            .await
            .expect("list should succeed");

        let files = result["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].as_str().unwrap(), "readme.md");

        let folders = result["folders"].as_array().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].as_str().unwrap(), "wiki");

        assert_eq!(result["count"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_page_list_fns_error() {
        let (fns, _index, server) = setup_test_fns_and_index().await;

        Mock::given(method("GET"))
            .and(path("/api/folder/notes"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/folders"))
            .and(query_param("vault", "test-vault"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": []
            })))
            .mount(&server)
            .await;

        let result = handle_page_list(&fns, None).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Fns(_)));
    }

    #[tokio::test]
    async fn test_page_put_new_page_requires_frontmatter() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "new-page";
        let body = "Some content.\n";

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_put(
            &fns,
            &index,
            slug,
            body,
            None,
            sample_timeline_entry(),
            None,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("frontmatter is required"),
            "expected frontmatter error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_page_put_new_page_requires_title() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "new-page";
        let body = "Some content.\n";
        let fm_no_title = serde_json::json!({"page_type": "Entity"});

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_put(
            &fns,
            &index,
            slug,
            body,
            Some(&fm_no_title),
            sample_timeline_entry(),
            None,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("title is required"),
            "expected title error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_page_put_new_page_rejects_empty_title() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "new-page";
        let body = "Some content.\n";
        let fm_empty_title = serde_json::json!({"title": ""});

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .expect(1)
            .mount(&server)
            .await;

        let result = handle_page_put(
            &fns,
            &index,
            slug,
            body,
            Some(&fm_empty_title),
            sample_timeline_entry(),
            None,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("title must not be empty"),
            "expected empty title error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_page_put_update_existing_merges_frontmatter() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "existing-page";
        let existing_content = "---\ntitle: Old Title\npage_type: Concept\ntags:\n  - rust\nsources: []\n---\nOld body.\n---\n- 2024-01-01: Old entry\n";

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": existing_content, "path": slug, "fileLinks": {}, "version": 1 }
            })))
            .expect(1)
            .mount(&server)
            .await;

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

        let updates = serde_json::json!({"title": "New Title", "visibility": "private"});
        let timeline = TimelineAppendInput {
            content: "Updated the title".into(),
            agent: Some("claude".into()),
        };

        let result = handle_page_put(
            &fns,
            &index,
            slug,
            "New body.\n",
            Some(&updates),
            timeline,
            None,
        )
        .await
        .expect("put should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert_eq!(result["timeline_count"].as_u64().unwrap(), 2);
        assert!(result["content_hash"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_page_put_update_existing_no_frontmatter_merges_empty() {
        let (fns, index, server) = setup_test_fns_and_index().await;
        let slug = "existing-page";
        let existing_content = "---\ntitle: Keep Title\npage_type: Concept\ntags:\n  - rust\nsources: []\n---\nOld body.\n---\n- 2024-01-01: Old entry\n";

        Mock::given(method("GET"))
            .and(path("/api/note"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": { "content": existing_content, "path": slug, "fileLinks": {}, "version": 1 }
            })))
            .expect(1)
            .mount(&server)
            .await;

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

        let timeline = TimelineAppendInput {
            content: "Body only update".into(),
            agent: None,
        };

        let result = handle_page_put(&fns, &index, slug, "New body only.\n", None, timeline, None)
            .await
            .expect("put should succeed");

        assert_eq!(result["slug"].as_str().unwrap(), slug);
        assert_eq!(result["timeline_count"].as_u64().unwrap(), 2);
    }
}
