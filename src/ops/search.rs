use crate::graph;
use crate::index::IndexEngine;
use crate::search::keyword;
use crate::types::Result;
use serde_json::json;

pub async fn handle_search(
    index: &IndexEngine,
    query: &str,
    limit: Option<i64>,
    type_filter: Option<&str>,
    sort: Option<&str>,
) -> Result<serde_json::Value> {
    let limit_val = limit.unwrap_or(10);
    let results =
        keyword::keyword_search(index.pool(), query, limit_val, type_filter, sort).await?;

    let results_json: Vec<serde_json::Value> = results
        .iter()
        .map(|hit| {
            json!({
                "slug": hit.slug,
                "title": hit.title,
                "preview": hit.preview,
                "rank": hit.rank
            })
        })
        .collect();

    Ok(json!({
        "query": query,
        "total": results.len(),
        "results": results_json
    }))
}

pub async fn handle_graph_query(
    index: &IndexEngine,
    slug: &str,
    depth: Option<usize>,
    link_type: Option<&str>,
    direction: Option<&str>,
) -> Result<serde_json::Value> {
    let depth_val = depth.unwrap_or(1);
    let dir = direction.unwrap_or("out");

    let direct_links = match dir {
        "in" => graph::get_backlinks(index.pool(), slug, link_type).await?,
        "both" => {
            let mut out = graph::get_outlinks(index.pool(), slug, link_type).await?;
            let mut incoming = graph::get_backlinks(index.pool(), slug, link_type).await?;
            out.append(&mut incoming);
            out
        }
        _ => graph::get_outlinks(index.pool(), slug, link_type).await?,
    };

    let neighbors =
        graph::get_neighbors(index.pool(), slug, depth_val, link_type, direction).await?;

    let outlinks_json: Vec<serde_json::Value> = direct_links
        .iter()
        .map(|link| {
            json!({
                "source_slug": link.source_slug,
                "target_slug": link.target_slug,
                "link_type": format!("{:?}", link.link_type),
                "context_snippet": link.context_snippet
            })
        })
        .collect();

    let neighbors_json: Vec<serde_json::Value> = neighbors
        .iter()
        .map(|(slug, distance)| {
            json!({
                "slug": slug,
                "distance": distance
            })
        })
        .collect();

    Ok(json!({
        "slug": slug,
        "outlinks": outlinks_json,
        "neighbors": neighbors_json
    }))
}

pub async fn handle_stats(index: &IndexEngine) -> Result<serde_json::Value> {
    let stats = index.get_stats().await?;

    Ok(json!({
        "total_pages": stats.total_pages,
        "pages_by_type": stats.pages_by_type,
        "total_links": stats.total_links,
        "orphan_count": stats.orphan_count
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use crate::types::{Link, LinkType, PageType};

    #[tokio::test]
    async fn test_search_integration() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        let page = sample_page(
            "rust-page",
            "Rust Programming Language",
            PageType::Concept,
            "Rust is great for systems programming.",
        );
        engine.index_page(&page).await.unwrap();

        let result = handle_search(&engine, "systems", Some(10), None, None)
            .await
            .unwrap();

        assert_eq!(result["query"], "systems");
        assert_eq!(result["total"], 1);
        assert_eq!(result["results"].as_array().unwrap().len(), 1);
        assert_eq!(result["results"][0]["slug"], "rust-page");
    }

    #[tokio::test]
    async fn test_search_empty() {
        let engine = IndexEngine::new(":memory:").await.unwrap();
        let page = sample_page("test", "Test", PageType::Concept, "Content");
        engine.index_page(&page).await.unwrap();

        let result = handle_search(&engine, "", Some(10), None, None)
            .await
            .unwrap();

        assert_eq!(result["query"], "");
        assert_eq!(result["total"], 0);
        assert!(result["results"].as_array().unwrap().is_empty());

        let result = handle_search(&engine, "   ", Some(10), None, None)
            .await
            .unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["results"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_graph_query() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Entity, "A content");
        let page_b = sample_page("page-b", "Page B", PageType::Concept, "B content");
        let page_c = sample_page("page-c", "Page C", PageType::Source, "C content");

        engine.index_page(&page_a).await.unwrap();
        engine.index_page(&page_b).await.unwrap();
        engine.index_page(&page_c).await.unwrap();

        let links = vec![
            Link {
                source_slug: "page-a".to_string(),
                target_slug: "page-b".to_string(),
                link_type: LinkType::Plain,
                context_snippet: Some("see also".to_string()),
            },
            Link {
                source_slug: "page-a".to_string(),
                target_slug: "page-c".to_string(),
                link_type: LinkType::Custom("cites".to_string()),
                context_snippet: None,
            },
        ];
        engine.update_links("page-a", &links).await.unwrap();

        let result = handle_graph_query(&engine, "page-a", Some(1), None, None)
            .await
            .unwrap();

        assert_eq!(result["slug"], "page-a");
        assert_eq!(result["outlinks"].as_array().unwrap().len(), 2);
        assert_eq!(result["neighbors"].as_array().unwrap().len(), 2);

        let outlink_slugs: Vec<String> = result["outlinks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["target_slug"].as_str().unwrap().to_string())
            .collect();
        assert!(outlink_slugs.contains(&"page-b".to_string()));
        assert!(outlink_slugs.contains(&"page-c".to_string()));
    }

    #[tokio::test]
    async fn test_graph_query_with_direction_in() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Entity, "A content");
        let page_b = sample_page("page-b", "Page B", PageType::Concept, "B content");
        let page_c = sample_page("page-c", "Page C", PageType::Source, "C content");

        engine.index_page(&page_a).await.unwrap();
        engine.index_page(&page_b).await.unwrap();
        engine.index_page(&page_c).await.unwrap();

        let links = [
            Link {
                source_slug: "page-b".to_string(),
                target_slug: "page-a".to_string(),
                link_type: LinkType::Plain,
                context_snippet: None,
            },
            Link {
                source_slug: "page-c".to_string(),
                target_slug: "page-a".to_string(),
                link_type: LinkType::Custom("references".to_string()),
                context_snippet: None,
            },
        ];
        engine.update_links("page-b", &links[0..1]).await.unwrap();
        engine.update_links("page-c", &links[1..2]).await.unwrap();

        let result = handle_graph_query(&engine, "page-a", Some(1), None, Some("in"))
            .await
            .unwrap();

        assert_eq!(result["outlinks"].as_array().unwrap().len(), 2);
        assert_eq!(result["neighbors"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_graph_query_with_link_type_filter() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page_a = sample_page("page-a", "Page A", PageType::Entity, "A content");
        let page_b = sample_page("page-b", "Page B", PageType::Concept, "B content");
        let page_c = sample_page("page-c", "Page C", PageType::Source, "C content");

        engine.index_page(&page_a).await.unwrap();
        engine.index_page(&page_b).await.unwrap();
        engine.index_page(&page_c).await.unwrap();

        let links = vec![
            Link {
                source_slug: "page-a".to_string(),
                target_slug: "page-b".to_string(),
                link_type: LinkType::Plain,
                context_snippet: None,
            },
            Link {
                source_slug: "page-a".to_string(),
                target_slug: "page-c".to_string(),
                link_type: LinkType::Custom("cites".to_string()),
                context_snippet: None,
            },
        ];
        engine.update_links("page-a", &links).await.unwrap();

        let result = handle_graph_query(&engine, "page-a", Some(1), Some("plain"), None)
            .await
            .unwrap();

        assert_eq!(result["outlinks"].as_array().unwrap().len(), 1);
        assert_eq!(result["outlinks"][0]["target_slug"], "page-b");
        assert_eq!(result["neighbors"].as_array().unwrap().len(), 1);
        assert_eq!(result["neighbors"][0]["slug"], "page-b");
    }

    #[tokio::test]
    async fn test_stats() {
        let engine = IndexEngine::new(":memory:").await.unwrap();

        let page1 = sample_page("page-1", "Page 1", PageType::Entity, "Content 1");
        let page2 = sample_page("page-2", "Page 2", PageType::Concept, "Content 2");
        let page3 = sample_page("page-3", "Page 3", PageType::Concept, "Content 3");

        engine.index_page(&page1).await.unwrap();
        engine.index_page(&page2).await.unwrap();
        engine.index_page(&page3).await.unwrap();

        let links = vec![
            Link {
                source_slug: "page-1".to_string(),
                target_slug: "page-2".to_string(),
                link_type: LinkType::Plain,
                context_snippet: None,
            },
            Link {
                source_slug: "page-1".to_string(),
                target_slug: "page-3".to_string(),
                link_type: LinkType::Plain,
                context_snippet: None,
            },
        ];
        engine.update_links("page-1", &links).await.unwrap();

        let result = handle_stats(&engine).await.unwrap();

        assert_eq!(result["total_pages"], 3);
        assert_eq!(result["total_links"], 2);
        assert_eq!(result["pages_by_type"]["Entity"].as_i64(), Some(1));
        assert_eq!(result["pages_by_type"]["Concept"].as_i64(), Some(2));
    }
}
