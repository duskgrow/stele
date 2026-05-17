use crate::fns::FnsClient;
use crate::graph;
use crate::index::IndexEngine;
use crate::ops::sync;
use crate::parser::page::validate_slug;
use crate::types::{Error, Result};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct MaintainIssue {
    pub severity: String,
    pub scope: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct MaintainResult {
    pub scope: String,
    pub issues_found: Vec<MaintainIssue>,
}

pub async fn handle_maintain(
    index: &IndexEngine,
    scope: Option<&str>,
) -> Result<serde_json::Value> {
    let scope = scope.unwrap_or("full");

    let mut all_issues = Vec::new();

    if scope == "lint" || scope == "full" {
        let lint_issues = run_lint(index).await?;
        all_issues.extend(lint_issues);
    }

    if scope == "orphans" || scope == "full" {
        let orphan_issues = run_orphans(index).await?;
        all_issues.extend(orphan_issues);
    }

    if scope == "backlinks" || scope == "full" {
        let backlink_issues = run_backlinks(index).await?;
        all_issues.extend(backlink_issues);
    }

    let issues_json: Vec<serde_json::Value> = all_issues
        .iter()
        .map(|issue| {
            json!({
                "severity": issue.severity,
                "scope": issue.scope,
                "message": issue.message,
            })
        })
        .collect();

    Ok(json!({
        "scope": scope,
        "issues_count": all_issues.len(),
        "issues": issues_json,
    }))
}

async fn run_lint(index: &IndexEngine) -> Result<Vec<MaintainIssue>> {
    let mut issues = Vec::new();
    let slugs = index.list_slugs().await?;

    for slug in slugs {
        if let Some(page) = index.get_page(&slug).await? {
            if page.frontmatter.title.trim().is_empty() {
                issues.push(MaintainIssue {
                    severity: "error".to_string(),
                    scope: "lint".to_string(),
                    message: format!("Page '{}' has empty title", slug),
                });
            }

            if page.compiled_truth.trim().is_empty() {
                issues.push(MaintainIssue {
                    severity: "warning".to_string(),
                    scope: "lint".to_string(),
                    message: format!("Page '{}' has empty compiled_truth", slug),
                });
            }

            if validate_slug(&slug).is_err() {
                issues.push(MaintainIssue {
                    severity: "error".to_string(),
                    scope: "lint".to_string(),
                    message: format!("Page '{}' has invalid slug format", slug),
                });
            }

            if page.frontmatter.page_type == crate::types::PageType::Source
                && page.frontmatter.sources.is_empty()
            {
                issues.push(MaintainIssue {
                    severity: "warning".to_string(),
                    scope: "lint".to_string(),
                    message: format!("Page '{}' (Source) has empty sources", slug),
                });
            }

            if page.frontmatter.tags.is_empty() {
                issues.push(MaintainIssue {
                    severity: "warning".to_string(),
                    scope: "lint".to_string(),
                    message: format!("Page '{}' has empty tags", slug),
                });
            }

            if page.timeline.is_empty() {
                issues.push(MaintainIssue {
                    severity: "warning".to_string(),
                    scope: "lint".to_string(),
                    message: format!("Page '{}' has empty timeline", slug),
                });
            }
        }
    }

    Ok(issues)
}

async fn run_orphans(index: &IndexEngine) -> Result<Vec<MaintainIssue>> {
    let orphans = graph::find_orphans(index.pool()).await?;
    let mut issues = Vec::new();

    for slug in orphans {
        issues.push(MaintainIssue {
            severity: "warning".to_string(),
            scope: "orphans".to_string(),
            message: format!("Page '{}' has no inbound links", slug),
        });
    }

    Ok(issues)
}

async fn run_backlinks(index: &IndexEngine) -> Result<Vec<MaintainIssue>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT source_slug, target_slug FROM links l WHERE NOT EXISTS (SELECT 1 FROM pages p WHERE p.slug = l.target_slug)",
    )
    .fetch_all(index.pool())
    .await
    .map_err(|e| Error::Storage(format!("broken_backlinks: {e}")))?;

    let mut issues = Vec::new();
    for (source_slug, target_slug) in rows {
        issues.push(MaintainIssue {
            severity: "error".to_string(),
            scope: "backlinks".to_string(),
            message: format!(
                "Page '{}' links to nonexistent page '{}'",
                source_slug, target_slug
            ),
        });
    }

    Ok(issues)
}

pub async fn handle_reindex(fns: &FnsClient, index: &IndexEngine) -> Result<serde_json::Value> {
    sqlx::query("DELETE FROM links")
        .execute(index.pool())
        .await
        .map_err(|e| Error::Storage(format!("delete links: {e}")))?;

    sqlx::query("DELETE FROM pages")
        .execute(index.pool())
        .await
        .map_err(|e| Error::Storage(format!("delete pages: {e}")))?;

    let sync_result = sync::handle_sync(fns, index, None).await?;

    Ok(json!({
        "reindexed": true,
        "result": sync_result,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use crate::types::{Link, LinkType, PageType};
    use std::collections::HashSet;

    #[tokio::test]
    async fn test_maintain_lint_empty_title() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let mut page = sample_page("test-page", "", PageType::Concept, "Some truth content");
        page.frontmatter.title = "".to_string();
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, Some("lint")).await.unwrap();
        assert_eq!(result["scope"], "lint");
        assert_eq!(result["issues_count"], 1);

        let issues = result["issues"].as_array().unwrap();
        assert_eq!(issues.len(), 1);

        let has_empty_title = issues.iter().any(|i| {
            i["scope"] == "lint"
                && i["severity"] == "error"
                && i["message"].as_str().unwrap().contains("empty title")
        });
        assert!(has_empty_title, "expected empty title lint issue");
    }

    #[tokio::test]
    async fn test_maintain_lint_valid_slugs_not_flagged() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let page = sample_page("wiki/bugs/issue", "Wiki Bugs", PageType::Concept, "Content");
        index.index_page(&page).await.unwrap();

        let page2 = sample_page("my-page", "My Page", PageType::Concept, "Content");
        index.index_page(&page2).await.unwrap();

        let result = handle_maintain(&index, Some("lint")).await.unwrap();
        let issues = result["issues"].as_array().unwrap();

        let has_slug_issue = issues.iter().any(|i| {
            i["scope"] == "lint"
                && i["message"]
                    .as_str()
                    .unwrap()
                    .contains("invalid slug format")
        });
        assert!(
            !has_slug_issue,
            "valid path-style slugs should not be flagged"
        );
    }

    #[tokio::test]
    async fn test_maintain_lint_invalid_slug_flagged() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let mut page = sample_page("bad_slug!", "Bad Slug", PageType::Concept, "Content");
        page.frontmatter.title = "".to_string();
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, Some("lint")).await.unwrap();
        let issues = result["issues"].as_array().unwrap();

        let has_slug_issue = issues.iter().any(|i| {
            i["scope"] == "lint"
                && i["severity"] == "error"
                && i["message"]
                    .as_str()
                    .unwrap()
                    .contains("invalid slug format")
        });
        assert!(has_slug_issue, "invalid slug should be flagged");
    }

    #[tokio::test]
    async fn test_maintain_lint_empty_sources_on_source_page() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let page = sample_page("source-page", "Source Page", PageType::Source, "Content");
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, Some("lint")).await.unwrap();
        let issues = result["issues"].as_array().unwrap();

        let has_empty_sources = issues.iter().any(|i| {
            i["scope"] == "lint"
                && i["severity"] == "warning"
                && i["message"].as_str().unwrap().contains("empty sources")
        });
        assert!(
            has_empty_sources,
            "expected empty sources lint issue for Source page"
        );
    }

    #[tokio::test]
    async fn test_maintain_lint_non_source_empty_sources_not_flagged() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let page = sample_page("concept-page", "Concept Page", PageType::Concept, "Content");
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, Some("lint")).await.unwrap();
        let issues = result["issues"].as_array().unwrap();

        let has_empty_sources = issues.iter().any(|i| {
            i["scope"] == "lint" && i["message"].as_str().unwrap().contains("empty sources")
        });
        assert!(
            !has_empty_sources,
            "non-Source pages should not be flagged for empty sources"
        );
    }

    #[tokio::test]
    async fn test_maintain_lint_empty_tags() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let mut page = sample_page("tagless-page", "Tagless Page", PageType::Concept, "Content");
        page.frontmatter.tags = vec![];
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, Some("lint")).await.unwrap();
        let issues = result["issues"].as_array().unwrap();

        let has_empty_tags = issues.iter().any(|i| {
            i["scope"] == "lint"
                && i["severity"] == "warning"
                && i["message"].as_str().unwrap().contains("empty tags")
        });
        assert!(has_empty_tags, "expected empty tags lint issue");
    }

    #[tokio::test]
    async fn test_maintain_lint_empty_timeline() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let mut page = sample_page("no-timeline", "No Timeline", PageType::Concept, "Content");
        page.timeline = vec![];
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, Some("lint")).await.unwrap();
        let issues = result["issues"].as_array().unwrap();

        let has_empty_timeline = issues.iter().any(|i| {
            i["scope"] == "lint"
                && i["severity"] == "warning"
                && i["message"].as_str().unwrap().contains("empty timeline")
        });
        assert!(has_empty_timeline, "expected empty timeline lint issue");
    }

    #[tokio::test]
    async fn test_maintain_orphans() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Concept, "Content A");
        let page_b = sample_page("page-b", "Page B", PageType::Concept, "Content B");
        let page_c = sample_page("page-c", "Page C", PageType::Concept, "Content C");

        index.index_page(&page_a).await.unwrap();
        index.index_page(&page_b).await.unwrap();
        index.index_page(&page_c).await.unwrap();

        let links = vec![Link {
            source_slug: "page-a".to_string(),
            target_slug: "page-b".to_string(),
            link_type: LinkType::Plain,
            context_snippet: None,
        }];
        index.update_links("page-a", &links).await.unwrap();

        let result = handle_maintain(&index, Some("orphans")).await.unwrap();
        assert_eq!(result["scope"], "orphans");

        let issues = result["issues"].as_array().unwrap();
        assert_eq!(issues.len(), 2);

        let orphan_slugs: HashSet<String> = issues
            .iter()
            .map(|i| {
                let msg = i["message"].as_str().unwrap();
                msg.split("'").nth(1).unwrap().to_string()
            })
            .collect();

        assert!(orphan_slugs.contains("page-a"));
        assert!(orphan_slugs.contains("page-c"));
    }

    #[tokio::test]
    async fn test_maintain_broken_backlinks() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Concept, "Content A");
        index.index_page(&page_a).await.unwrap();

        let links = vec![Link {
            source_slug: "page-a".to_string(),
            target_slug: "nonexistent-page".to_string(),
            link_type: LinkType::Plain,
            context_snippet: None,
        }];
        index.update_links("page-a", &links).await.unwrap();

        let result = handle_maintain(&index, Some("backlinks")).await.unwrap();
        assert_eq!(result["scope"], "backlinks");

        let issues = result["issues"].as_array().unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0]["severity"], "error");
        assert_eq!(issues[0]["scope"], "backlinks");
        assert!(
            issues[0]["message"]
                .as_str()
                .unwrap()
                .contains("nonexistent-page")
        );
    }

    #[tokio::test]
    async fn test_maintain_valid_backlinks_not_flagged() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Concept, "Content A");
        let page_b = sample_page("page-b", "Page B", PageType::Concept, "Content B");
        index.index_page(&page_a).await.unwrap();
        index.index_page(&page_b).await.unwrap();

        let links = vec![Link {
            source_slug: "page-a".to_string(),
            target_slug: "page-b".to_string(),
            link_type: LinkType::Plain,
            context_snippet: None,
        }];
        index.update_links("page-a", &links).await.unwrap();

        let result = handle_maintain(&index, Some("backlinks")).await.unwrap();
        let issues = result["issues"].as_array().unwrap();
        assert_eq!(
            issues.len(),
            0,
            "valid links should not be reported as broken"
        );
    }

    #[tokio::test]
    async fn test_maintain_full() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let mut page = sample_page("bad_slug!", "", PageType::Concept, "");
        page.frontmatter.title = "".to_string();
        page.compiled_truth = "".to_string();
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, Some("full")).await.unwrap();
        assert_eq!(result["scope"], "full");

        let issues = result["issues"].as_array().unwrap();
        assert!(!issues.is_empty(), "full scope should find issues");

        let scopes: HashSet<String> = issues
            .iter()
            .map(|i| i["scope"].as_str().unwrap().to_string())
            .collect();
        assert!(scopes.contains("lint"), "full should include lint issues");
        assert!(
            scopes.contains("orphans"),
            "full should include orphan issues"
        );
    }

    #[tokio::test]
    async fn test_reindex() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let index = IndexEngine::new(":memory:").await.unwrap();

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 1,
                "status": true,
                "message": "Success",
                "data": []
            })))
            .mount(&server)
            .await;

        let fns = FnsClient::new(
            server.uri(),
            "test-token".to_string(),
            "test-vault".to_string(),
        );

        let page = sample_page("test-page", "Test Page", PageType::Concept, "Content");
        index.index_page(&page).await.unwrap();

        assert!(index.get_page("test-page").await.unwrap().is_some());

        let result = handle_reindex(&fns, &index).await.unwrap();
        assert_eq!(result["reindexed"], true);
        assert!(result.get("result").is_some());

        assert!(index.get_page("test-page").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_maintain_default_scope_is_full() {
        let index = IndexEngine::new(":memory:").await.unwrap();

        let page = sample_page("page-a", "Page A", PageType::Concept, "Content A");
        index.index_page(&page).await.unwrap();

        let result = handle_maintain(&index, None).await.unwrap();
        assert_eq!(result["scope"], "full");
    }
}
