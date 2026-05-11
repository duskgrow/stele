use async_trait::async_trait;
use serde_json::{Value, json};
use crate::ops::handler::{OpHandler, OpExec, OperationContext};
use crate::types::TimelineAppendInput;

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

fn validate_page_type(frontmatter: &Option<Value>) -> Result<(), anyhow::Error> {
    if let Some(Value::Object(map)) = frontmatter {
        if let Some(page_type_val) = map.get("page_type") {
            if let Some(page_type_str) = page_type_val.as_str() {
                let valid = matches!(
                    page_type_str,
                    "Entity" | "Concept" | "Source" | "Query" | "Synthesis" | "Comparison"
                );
                if !valid {
                    return Err(anyhow::anyhow!(
                        "invalid page_type: '{}'. Valid types: Entity, Concept, Source, Query, Synthesis, Comparison",
                        page_type_str
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
    async fn execute(&self, ctx: &OperationContext) -> Result<Value, anyhow::Error> {
        validate_page_type(&self.frontmatter_updates)?;

        crate::ops::page::handle_page_put(
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
                        "page_type": { "type": "string", "enum": ["Entity", "Concept", "Source", "Query", "Synthesis", "Comparison"] },
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

    fn from_mcp_args(&self, args: Option<serde_json::Map<String, Value>>) -> Result<Box<dyn OpExec>, anyhow::Error> {
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

    fn from_cli_matches(&self, matches: &clap::ArgMatches) -> Result<Box<dyn OpExec>, anyhow::Error> {
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
    fn test_page_put_handler_meta() {
        let handler = PagePutHandler;
        assert_eq!(handler.name(), "page.put");
        assert!(!handler.description().is_empty());

        let schema = handler.input_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["slug"].is_object());
        assert!(schema["properties"]["body"].is_object());
        assert!(schema["properties"]["frontmatter"].is_object());
        assert!(schema["properties"]["timeline"].is_object());
        assert!(schema["properties"]["etag"].is_object());

        let required = schema["required"].as_array().unwrap();
        let required_fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_fields.contains(&"slug"));
        assert!(required_fields.contains(&"body"));
        assert!(required_fields.contains(&"timeline"));
    }

    #[test]
    fn test_page_put_from_mcp_args_full() {
        let handler = PagePutHandler;
        let mut args = serde_json::Map::new();
        args.insert("slug".to_string(), Value::String("test-page".to_string()));
        args.insert("body".to_string(), Value::String("Body content".to_string()));
        args.insert("frontmatter".to_string(), json!({"title": "Test", "status": "Budding"}));
        let mut timeline = serde_json::Map::new();
        timeline.insert("content".to_string(), Value::String("Updated".to_string()));
        timeline.insert("agent".to_string(), Value::String("claude".to_string()));
        args.insert("timeline".to_string(), Value::Object(timeline));
        args.insert("etag".to_string(), Value::String("abc123".to_string()));

        let exec = handler.from_mcp_args(Some(args)).expect("from_mcp_args should succeed");
        let _ = exec;
    }

    #[test]
    fn test_page_put_from_mcp_args_minimal() {
        let handler = PagePutHandler;
        let mut args = serde_json::Map::new();
        args.insert("slug".to_string(), Value::String("test-page".to_string()));
        args.insert("body".to_string(), Value::String("# Hello".to_string()));
        let mut timeline = serde_json::Map::new();
        timeline.insert("content".to_string(), Value::String("Created page".to_string()));
        args.insert("timeline".to_string(), Value::Object(timeline));

        let exec = handler.from_mcp_args(Some(args)).expect("from_mcp_args should succeed");
        let _ = exec;
    }

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
    fn test_page_put_in_inventory() {
        let handlers: Vec<_> = inventory::iter::<&'static dyn OpHandler>
            .into_iter()
            .collect();
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(names.contains(&"page.put"), "page.put should be in inventory, found: {:?}", names);
    }

    #[test]
    fn test_validate_page_type_all_valid_types() {
        let valid_types = ["Entity", "Concept", "Source", "Query", "Synthesis", "Comparison"];
        for pt in valid_types {
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
        assert!(err.contains("Valid types: Entity, Concept, Source, Query, Synthesis, Comparison"), "expected valid types list, got: {err}");
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
        ));
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
        assert!(err.contains("Valid types: Entity, Concept, Source, Query, Synthesis, Comparison"), "expected valid types list, got: {err}");
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
        ));
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
