use chrono::Utc;
use serde_json::{Value, json};
use tracing::warn;
use crate::fns::FnsClient;
use crate::index::IndexEngine;
use crate::parser::frontmatter;
use crate::parser::page as page_parser;
use crate::parser::wikilink;
use crate::types::{Error, Frontmatter, Page, PageType, TimelineAppendInput, TimelineEntry};

pub async fn handle_page_put(
    fns: &FnsClient,
    index: &IndexEngine,
    slug: &str,
    body: &str,
    frontmatter_updates: Option<&Value>,
    timeline_append: TimelineAppendInput,
    etag: Option<&str>,
) -> crate::types::Result<Value> {
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

use async_trait::async_trait;
use crate::ops::handler::{OpHandler, OpExec, OperationContext};

/// Handler struct registered with inventory.
pub struct PagePutHandler;

/// Executable operation with parsed arguments.
pub struct PagePutOp {
    pub slug: String,
    pub body: String,
    pub frontmatter_updates: Option<Value>,
    pub timeline_append: TimelineAppendInput,
    pub etag: Option<String>,
}

fn validate_page_type(frontmatter: &Option<Value>) -> std::result::Result<(), anyhow::Error> {
    if let Some(Value::Object(map)) = frontmatter {
        if let Some(page_type_val) = map.get("page_type") {
            if let Some(page_type_str) = page_type_val.as_str() {
                if !PageType::NAMES.contains(&page_type_str) {
                    return Err(anyhow::anyhow!(
                        "invalid page_type: '{}'. Valid types: {}",
                        page_type_str,
                        PageType::NAMES.join(", ")
                    ));
                }
            }
        }
    }
    Ok(())
}

#[async_trait]
impl OpExec for PagePutOp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn execute(&self, ctx: &OperationContext) -> std::result::Result<Value, anyhow::Error> {
        validate_page_type(&self.frontmatter_updates)?;

        handle_page_put(
            &ctx.fns,
            &ctx.index,
            &self.slug,
            &self.body,
            self.frontmatter_updates.as_ref(),
            self.timeline_append.clone(),
            self.etag.as_deref(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

impl OpHandler for PagePutHandler {
    fn name(&self) -> &'static str { "page.put" }
    fn description(&self) -> &'static str { "Create or update a page with structured input" }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "slug": { "type": "string" },
                "body": { "type": "string", "description": "Markdown body content (without frontmatter)" },
                "frontmatter": {
                    "type": "object",
                    "description": "Frontmatter fields to merge (required 'title' for new pages)",
                    "properties": {
                        "title": { "type": "string" },
                        "page_type": { "type": "string", "enum": PageType::NAMES },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "sources": { "type": "array", "items": { "type": "string" } },
                        "date": { "type": "string" },
                        "visibility": { "type": "string", "enum": ["shared", "private"], "default": "shared" },
                        "created_by": { "type": "string", "description": "Agent identifier" }
                    }
                },
                "timeline": {
                    "type": "object",
                    "description": "Timeline entry to append (date auto-generated)",
                    "properties": {
                        "content": { "type": "string" },
                        "agent": { "type": "string" }
                    },
                    "required": ["content"]
                },
                "etag": { "type": "string" }
            },
            "required": ["slug", "body", "timeline"]
        })
    }

    fn from_mcp_args(&self, args: Option<serde_json::Map<String, Value>>) -> std::result::Result<Box<dyn OpExec>, anyhow::Error> {
        let args = args.unwrap_or_default();

        let slug = args.get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required field: slug"))?
            .to_string();

        let body = args.get("body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required field: body"))?
            .to_string();

        let frontmatter_updates = args.get("frontmatter").cloned();

        let timeline_obj = args.get("timeline")
            .ok_or_else(|| anyhow::anyhow!("missing required field: timeline"))?;

        let timeline_content = timeline_obj
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("missing required field: timeline.content"))?;

        let timeline_agent = timeline_obj
            .get("agent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let etag = args.get("etag")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(Box::new(PagePutOp {
            slug,
            body,
            frontmatter_updates,
            timeline_append: TimelineAppendInput {
                content: timeline_content,
                agent: timeline_agent,
            },
            etag,
        }))
    }

    fn cli_command(&self) -> clap::Command {
        clap::Command::new("put")
            .about("Create or update a page")
            .arg(clap::Arg::new("slug").required(true))
            .arg(clap::Arg::new("content")
                .long("content")
                .value_name("TEXT")
                .help("Body content as text"))
            .arg(clap::Arg::new("file")
                .long("file")
                .value_name("PATH")
                .help("Body content from file"))
            .arg(clap::Arg::new("frontmatter")
                .long("frontmatter")
                .value_name("JSON")
                .help("Frontmatter updates as JSON"))
            .arg(clap::Arg::new("timeline-content")
                .long("timeline-content")
                .value_name("TEXT")
                .required(true)
                .help("Timeline entry content (required)"))
            .arg(clap::Arg::new("timeline-agent")
                .long("timeline-agent")
                .value_name("AGENT")
                .help("Timeline entry agent name"))
            .arg(clap::Arg::new("etag")
                .long("etag")
                .value_name("ETAG")
                .help("Expected content hash for optimistic concurrency"))
    }

    fn from_cli_matches(&self, matches: &clap::ArgMatches) -> std::result::Result<Box<dyn OpExec>, anyhow::Error> {
        let slug = matches.get_one::<String>("slug")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: slug"))?
            .clone();

        let body = match (matches.get_one::<String>("file"), matches.get_one::<String>("content")) {
            (Some(path), None) => std::fs::read_to_string(path)?,
            (None, Some(text)) => text.clone(),
            (Some(_), Some(_)) => {
                return Err(anyhow::anyhow!("cannot specify both --file and --content"));
            }
            (None, None) => {
                return Err(anyhow::anyhow!("must specify either --file or --content"));
            }
        };

        let frontmatter_updates = match matches.get_one::<String>("frontmatter") {
            Some(json_str) => {
                let val: Value = serde_json::from_str(json_str)?;
                Some(val)
            }
            None => None,
        };

        let timeline_content = matches.get_one::<String>("timeline-content")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: timeline-content"))?
            .clone();

        let timeline_agent = matches.get_one::<String>("timeline-agent").cloned();

        let etag = matches.get_one::<String>("etag").cloned();

        Ok(Box::new(PagePutOp {
            slug,
            body,
            frontmatter_updates,
            timeline_append: TimelineAppendInput {
                content: timeline_content,
                agent: timeline_agent,
            },
            etag,
        }))
    }
}

inventory::submit! {
    &PagePutHandler as &'static dyn OpHandler
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_page_put_missing_timeline() {
        let handler = PagePutHandler;
        let mut args = serde_json::Map::new();
        args.insert("slug".to_string(), Value::String("test-page".to_string()));
        args.insert("body".to_string(), Value::String("# Hello".to_string()));

        let result = handler.from_mcp_args(Some(args));
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(err.contains("missing required field: timeline"), "expected timeline error, got: {err}");
    }

    #[test]
    fn test_validate_page_type_all_valid_types() {
        for pt in PageType::NAMES {
            let fm = Some(json!({"page_type": pt}));
            assert!(
                validate_page_type(&fm).is_ok(),
                "expected '{}' to be valid",
                pt
            );
        }
    }

    #[test]
    fn test_validate_page_type_invalid_rejected() {
        let fm = Some(json!({"page_type": "InvalidType"}));
        let result = validate_page_type(&fm);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid page_type: 'InvalidType'"), "expected invalid page_type error, got: {err}");
        assert!(err.contains(&format!("Valid types: {}", PageType::NAMES.join(", "))), "expected valid types list, got: {err}");
    }

    #[test]
    fn test_validate_page_type_missing_passes() {
        let fm = Some(json!({"title": "Test"}));
        assert!(validate_page_type(&fm).is_ok());
    }

    #[test]
    fn test_validate_page_type_none_passes() {
        assert!(validate_page_type(&None).is_ok());
    }

    #[tokio::test]
    async fn test_page_put_execute_invalid_page_type_rejected() {
        let server = wiremock::MockServer::start().await;
        let fns = Arc::new(crate::fns::FnsClient::new(
            server.uri(),
            "test-token".into(),
            "test-vault".into(),
        ).unwrap());
        let index = Arc::new(
            crate::index::IndexEngine::new("sqlite::memory:")
                .await
                .expect("in-memory index"),
        );
        let config = crate::config::Config::default();
        let ctx = crate::ops::handler::OperationContext { fns, index, config };

        let op = PagePutOp {
            slug: "test-page".to_string(),
            body: "Body".to_string(),
            frontmatter_updates: Some(json!({"page_type": "InvalidType"})),
            timeline_append: TimelineAppendInput {
                content: "entry".to_string(),
                agent: None,
            },
            etag: None,
        };

        let result = op.execute(&ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid page_type: 'InvalidType'"), "expected invalid page_type error, got: {err}");
        assert!(err.contains(&format!("Valid types: {}", PageType::NAMES.join(", "))), "expected valid types list, got: {err}");
    }

    #[tokio::test]
    async fn test_page_put_execute_valid_page_type_accepted() {
        let server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/note"))
            .respond_with(wiremock::ResponseTemplate::new(404).set_body_string("not found"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/note"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let fns = Arc::new(crate::fns::FnsClient::new(
            server.uri(),
            "test-token".into(),
            "test-vault".into(),
        ).unwrap());
        let index = Arc::new(
            crate::index::IndexEngine::new("sqlite::memory:")
                .await
                .expect("in-memory index"),
        );
        let config = crate::config::Config::default();
        let ctx = crate::ops::handler::OperationContext { fns, index, config };

        let op = PagePutOp {
            slug: "test-page".to_string(),
            body: "Body".to_string(),
            frontmatter_updates: Some(json!({
                "title": "Test",
                "page_type": "Concept",
                "tags": [],
                "sources": []
            })),
            timeline_append: TimelineAppendInput {
                content: "entry".to_string(),
                agent: None,
            },
            etag: None,
        };

        let result = op.execute(&ctx).await;
        assert!(result.is_ok(), "expected success, got: {:?}", result);
    }
}
