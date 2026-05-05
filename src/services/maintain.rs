use anyhow::Context;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::models::Frontmatter;
use crate::storage::sqlite::SqliteBackend;

/// Result of a brain_maintain operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintainResult {
    pub scope: String,
    pub issues_found: usize,
    pub details: serde_json::Value,
}

/// Run maintenance checks against the brain database.
///
/// * `lint`    – frontmatter completeness + slug naming  
/// * `orphans` – links whose `target_slug` has no matching page  
/// * `backlinks` – pages with zero incoming links  
/// * `full`    – all of the above  
pub async fn brain_maintain(sqlite: &SqliteBackend, scope: &str) -> anyhow::Result<MaintainResult> {
    match scope {
        "lint" => check_frontmatter_completeness(sqlite).await,
        "orphans" => check_orphan_links(sqlite).await,
        "backlinks" => check_backlink_mismatches(sqlite).await,
        "full" => run_all_checks(sqlite).await,
        _ => Err(anyhow::anyhow!("invalid scope: {}", scope)),
    }
}

async fn check_frontmatter_completeness(sqlite: &SqliteBackend) -> anyhow::Result<MaintainResult> {
    let pages = sqlite
        .list_all_pages()
        .await
        .context("listing pages for lint")?;

    let slug_re = Regex::new(r"^[a-z0-9/_-]+\.md$").unwrap();
    let mut missing_frontmatter = 0usize;
    let mut naming_violations = 0usize;
    let mut detail_list = Vec::new();

    for page in &pages {
        let mut issues = Vec::new();

        if page.title.trim().is_empty() {
            issues.push("missing title");
        }
        if page.page_type.trim().is_empty() {
            issues.push("missing type");
        }

        let fm_parse: Result<Frontmatter, _> = serde_json::from_str(&page.frontmatter);
        if fm_parse.is_err() {
            issues.push("unparseable frontmatter");
        } else if let Ok(ref fm) = fm_parse {
            if fm.date.to_string().is_empty() {
                issues.push("missing date");
            }
        }

        if !issues.is_empty() {
            missing_frontmatter += 1;
            detail_list.push(json!({
                "slug": page.slug,
                "issues": issues,
            }));
        }

        if !slug_re.is_match(&page.slug) {
            naming_violations += 1;
            detail_list.push(json!({
                "slug": page.slug,
                "issues": ["naming violation"],
            }));
        }
    }

    let issues_found = missing_frontmatter + naming_violations;
    Ok(MaintainResult {
        scope: "lint".into(),
        issues_found,
        details: json!({
            "missing_frontmatter": missing_frontmatter,
            "naming_violations": naming_violations,
            "pages_checked": pages.len(),
            "details": detail_list,
        }),
    })
}

async fn check_orphan_links(sqlite: &SqliteBackend) -> anyhow::Result<MaintainResult> {
    let orphans = sqlite
        .list_orphan_links()
        .await
        .context("listing orphan links")?;

    let issues_found = orphans.len();
    let detail_list: Vec<_> = orphans
        .iter()
        .map(|link| {
            json!({
                "source_slug": link.source_slug,
                "target_slug": link.target_slug,
                "link_type": link.link_type,
            })
        })
        .collect();

    Ok(MaintainResult {
        scope: "orphans".into(),
        issues_found,
        details: json!({
            "orphan_links": issues_found,
            "details": detail_list,
        }),
    })
}

async fn check_backlink_mismatches(sqlite: &SqliteBackend) -> anyhow::Result<MaintainResult> {
    let pages = sqlite
        .list_all_pages()
        .await
        .context("listing pages for backlink check")?;

    let mut broken_backlinks = 0usize;
    let mut detail_list = Vec::new();

    for page in &pages {
        let backlinks = sqlite
            .get_backlinks(&page.slug)
            .await
            .with_context(|| format!("fetching backlinks for {}", page.slug))?;

        if backlinks.is_empty() {
            broken_backlinks += 1;
            detail_list.push(json!({
                "slug": page.slug,
                "incoming_links": 0,
            }));
        }
    }

    Ok(MaintainResult {
        scope: "backlinks".into(),
        issues_found: broken_backlinks,
        details: json!({
            "broken_backlinks": broken_backlinks,
            "pages_checked": pages.len(),
            "details": detail_list,
        }),
    })
}

async fn run_all_checks(sqlite: &SqliteBackend) -> anyhow::Result<MaintainResult> {
    let lint = check_frontmatter_completeness(sqlite).await?;
    let orphans = check_orphan_links(sqlite).await?;
    let backlinks = check_backlink_mismatches(sqlite).await?;

    let issues_found = lint.issues_found + orphans.issues_found + backlinks.issues_found;

    let lint_details = lint.details;
    let orphan_details = orphans.details;
    let backlink_details = backlinks.details;

    Ok(MaintainResult {
        scope: "full".into(),
        issues_found,
        details: json!({
            "missing_frontmatter": lint_details["missing_frontmatter"],
            "orphan_links": orphan_details["orphan_links"],
            "broken_backlinks": backlink_details["broken_backlinks"],
            "naming_violations": lint_details["naming_violations"],
            "lint_details": lint_details,
            "orphan_details": orphan_details,
            "backlink_details": backlink_details,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Link, Page, PageType};
    use chrono::NaiveDate;

    fn sample_page(slug: &str, title: &str, page_type: PageType) -> Page {
        Page {
            slug: slug.to_string(),
            vault: "forge".to_string(),
            frontmatter: crate::models::Frontmatter {
                r#type: page_type,
                title: title.to_string(),
                tags: vec!["test".to_string()],
                related: vec![],
                sources: vec![],
                date: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
                status: None,
            },
            compiled_truth: "Some compiled truth content about this page.".to_string(),
            timeline: vec![],
            content_hash: "abc123".to_string(),
            raw_content: "# Raw".to_string(),
        }
    }

    async fn in_memory_backend() -> SqliteBackend {
        SqliteBackend::new(":memory:")
            .await
            .expect("creating in-memory backend")
    }

    #[tokio::test]
    async fn test_maintain_lint_clean() {
        let backend = in_memory_backend().await;
        let page = sample_page("wiki/test.md", "Test Page", PageType::Entity);
        backend.index_page(&page).await.unwrap();

        let result = brain_maintain(&backend, "lint").await.unwrap();
        assert_eq!(result.scope, "lint");
        assert_eq!(result.issues_found, 0);
        assert_eq!(result.details["missing_frontmatter"], 0);
        assert_eq!(result.details["naming_violations"], 0);
    }

    #[tokio::test]
    async fn test_maintain_lint_naming_violation() {
        let backend = in_memory_backend().await;
        let page = sample_page("wiki/BadName.md", "Bad Name", PageType::Entity);
        backend.index_page(&page).await.unwrap();

        let result = brain_maintain(&backend, "lint").await.unwrap();
        assert!(result.issues_found >= 1);
        assert!(result.details["naming_violations"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_maintain_orphans_finds_broken_link() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/exists.md", "Exists", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
                "wiki/exists.md",
                &[Link {
                    source_slug: "wiki/exists.md".to_string(),
                    target_slug: "wiki/missing.md".to_string(),
                    link_type: "link".to_string(),
                    context_snippet: None,
                }],
            )
            .await
            .unwrap();

        let result = brain_maintain(&backend, "orphans").await.unwrap();
        assert_eq!(result.issues_found, 1);
        assert_eq!(result.details["orphan_links"], 1);
    }

    #[tokio::test]
    async fn test_maintain_orphans_none() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/a.md", "A", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/b.md", "B", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
                "wiki/a.md",
                &[Link {
                    source_slug: "wiki/a.md".to_string(),
                    target_slug: "wiki/b.md".to_string(),
                    link_type: "link".to_string(),
                    context_snippet: None,
                }],
            )
            .await
            .unwrap();

        let result = brain_maintain(&backend, "orphans").await.unwrap();
        assert_eq!(result.issues_found, 0);
    }

    #[tokio::test]
    async fn test_maintain_backlinks_finds_isolated() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/a.md", "A", PageType::Entity))
            .await
            .unwrap();

        let result = brain_maintain(&backend, "backlinks").await.unwrap();
        assert_eq!(result.issues_found, 1);
        assert_eq!(result.details["broken_backlinks"], 1);
    }

    #[tokio::test]
    async fn test_maintain_backlinks_with_link() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/a.md", "A", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/b.md", "B", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
                "wiki/a.md",
                &[Link {
                    source_slug: "wiki/a.md".to_string(),
                    target_slug: "wiki/b.md".to_string(),
                    link_type: "link".to_string(),
                    context_snippet: None,
                }],
            )
            .await
            .unwrap();

        let result = brain_maintain(&backend, "backlinks").await.unwrap();
        assert_eq!(result.issues_found, 1);
        assert_eq!(result.details["broken_backlinks"], 1);
    }

    #[tokio::test]
    async fn test_maintain_full() {
        let backend = in_memory_backend().await;

        backend
            .index_page(&sample_page("wiki/a.md", "A", PageType::Entity))
            .await
            .unwrap();
        backend
            .index_page(&sample_page("wiki/b.md", "B", PageType::Entity))
            .await
            .unwrap();

        backend
            .update_links(
                "wiki/a.md",
                &[Link {
                    source_slug: "wiki/a.md".to_string(),
                    target_slug: "wiki/missing.md".to_string(),
                    link_type: "link".to_string(),
                    context_snippet: None,
                }],
            )
            .await
            .unwrap();

        let result = brain_maintain(&backend, "full").await.unwrap();
        assert_eq!(result.scope, "full");
        assert!(result.issues_found >= 2);
    }

    #[tokio::test]
    async fn test_maintain_invalid_scope() {
        let backend = in_memory_backend().await;
        let result = brain_maintain(&backend, "bogus").await;
        assert!(result.is_err());
    }
}
