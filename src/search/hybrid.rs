use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde_json::{Value, json};
use tracing::warn;

use crate::models::SearchSignals;
use crate::search::keyword::keyword_search;
use crate::search::vector::VectorSearch;
use crate::storage::sqlite::{SqliteBackend, truncate_str};

const RRF_K: f64 = 60.0;

pub fn rrf_score(rank: usize, weight: f64) -> f64 {
    weight / (RRF_K + rank as f64)
}

pub fn type_affinity(result_type: &str, preferred_type: Option<&str>) -> f64 {
    let pref = match preferred_type {
        Some(p) => p,
        None => return 1.0,
    };

    match (result_type, pref) {
        (a, b) if a == b => match a {
            "entity" | "concept" => 1.2,
            "source" => 0.5,
            _ => 1.0,
        },
        ("entity", "concept") | ("concept", "entity") => 1.0,
        ("entity", "source") | ("source", "entity") => 0.8,
        ("concept", "source") | ("source", "concept") => 0.8,
        _ => 1.0,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Candidate {
    keyword_rank: Option<usize>,
    vector_rank: Option<usize>,
    direct_link: bool,
    source_overlap: usize,
    common_neighbors: usize,
    result_type: String,
}

pub struct QueryContext {
    pub preferred_type: Option<String>,
}

impl QueryContext {
    pub fn new(preferred_type: Option<String>) -> Self {
        Self { preferred_type }
    }
}

fn compute_signals(candidate: &Candidate, ctx: &QueryContext) -> SearchSignals {
    SearchSignals {
        keyword_rank: candidate.keyword_rank,
        vector_rank: candidate.vector_rank,
        direct_link: candidate.direct_link,
        source_overlap: candidate.source_overlap,
        common_neighbors: candidate.common_neighbors,
        type_affinity: type_affinity(&candidate.result_type, ctx.preferred_type.as_deref()),
    }
}

pub fn hybrid_score(candidate: &Candidate, ctx: &QueryContext) -> f64 {
    let mut score = 0.0;

    if let Some(rank) = candidate.keyword_rank {
        score += rrf_score(rank, 1.0);
    }

    if let Some(rank) = candidate.vector_rank {
        score += rrf_score(rank, 1.0);
    }

    if candidate.direct_link {
        score += rrf_score(1, 3.0);
    }

    if candidate.source_overlap > 0 {
        score += rrf_score(1, 4.0 * candidate.source_overlap as f64);
    }

    if candidate.common_neighbors > 0 {
        score += rrf_score(1, 1.5 * candidate.common_neighbors as f64);
    }

    let affinity = type_affinity(&candidate.result_type, ctx.preferred_type.as_deref());
    score *= affinity;

    score
}

pub async fn brain_query(
    db: &SqliteBackend,
    vector_search: Option<&VectorSearch>,
    query: &str,
    limit: usize,
    from_slugs: &[String],
) -> Result<Value> {
    let search_limit = limit * 2;

    let keyword_hits = keyword_search(db, query, search_limit, None)
        .await
        .unwrap_or_default();

    let vector_hits: Vec<crate::search::vector::VectorHit> = match vector_search {
        Some(vs) if vs.is_enabled() => {
            warn!("vector search requires query embedding — skipping for text-only brain_query");
            Vec::new()
        }
        _ => Vec::new(),
    };

    let mut candidates: HashMap<String, Candidate> = HashMap::new();

    for (rank, hit) in keyword_hits.iter().enumerate() {
        let entry = candidates.entry(hit.slug.clone()).or_default();
        entry.keyword_rank = Some(rank + 1);
    }

    for (rank, hit) in vector_hits.iter().enumerate() {
        let entry = candidates.entry(hit.slug.clone()).or_default();
        entry.vector_rank = Some(rank + 1);
    }

    let candidate_slugs: Vec<String> = candidates.keys().cloned().collect();
    for slug in &candidate_slugs {
        match db.get_page(slug).await {
            Ok(Some(page)) => {
                if let Some(c) = candidates.get_mut(slug) {
                    c.result_type = page.page_type.clone();
                }
            }
            Ok(None) => {}
            Err(e) => warn!(slug = %slug, error = %e, "get_page failed during hybrid scoring"),
        }
    }

    let ctx = QueryContext::new(None);

    for slug in &candidate_slugs {
        let Some(candidate) = candidates.get_mut(slug) else {
            continue;
        };

        for from_slug in from_slugs {
            if db.has_direct_link(from_slug, slug).await.unwrap_or(false) {
                candidate.direct_link = true;
                break;
            }
        }

        if !from_slugs.is_empty() {
            let result_sources_raw = db.get_sources_for_page(slug).await.unwrap_or_default();
            let result_sources: HashSet<String> = result_sources_raw.into_iter().collect();

            let mut max_overlap = 0usize;
            for from_slug in from_slugs {
                let from_sources = db.get_sources_for_page(from_slug).await.unwrap_or_default();
                let overlap = from_sources
                    .iter()
                    .filter(|s| result_sources.contains(*s))
                    .count();
                max_overlap = max_overlap.max(overlap);
            }
            candidate.source_overlap = max_overlap;
        }

        if !from_slugs.is_empty() {
            let result_targets: HashSet<String> = db
                .get_outgoing_link_targets(slug)
                .await
                .unwrap_or_default()
                .into_iter()
                .collect();

            let mut common_count = 0usize;
            for from_slug in from_slugs {
                let from_targets: HashSet<String> = db
                    .get_outgoing_link_targets(from_slug)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .collect();

                for shared in result_targets.intersection(&from_targets) {
                    let degree = db.outgoing_degree(shared).await.unwrap_or(1);
                    let degree = degree.max(2);
                    common_count += (1.0_f64 / (degree as f64).ln()).ceil() as usize;
                }
            }
            candidate.common_neighbors = common_count;
        }
    }

    let mut scored: Vec<(String, f64, SearchSignals)> = candidates
        .into_iter()
        .map(|(slug, c)| {
            let score = hybrid_score(&c, &ctx);
            let signals = compute_signals(&c, &ctx);
            (slug, score, signals)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top_slugs: Vec<(String, f64, SearchSignals)> = scored
        .into_iter()
        .take(limit)
        .collect();

    // Load compiled_truth preview for each result
    let mut top: Vec<Value> = Vec::with_capacity(top_slugs.len());
    for (slug, score, signals) in top_slugs {
        let preview = match db.get_page(&slug).await {
            Ok(Some(row)) => row
                .compiled_truth
                .map(|t| truncate_str(&t, 500))
                .unwrap_or_default(),
            _ => String::new(),
        };

        top.push(json!({
            "slug": slug,
            "compiled_truth_preview": preview,
            "score": (score * 10000.0).round() / 10000.0,
            "signals": {
                "keyword_rank": signals.keyword_rank,
                "vector_rank": signals.vector_rank,
                "direct_link": signals.direct_link,
                "source_overlap": signals.source_overlap,
                "common_neighbors": signals.common_neighbors,
                "type_affinity": (signals.type_affinity * 100.0).round() / 100.0,
            }
        }));
    }

    Ok(json!({
        "query": query,
        "total": top.len(),
        "results": top
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_score_rank_1_weight_1() {
        let s = rrf_score(1, 1.0);
        assert!((s - 1.0 / 61.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rrf_score_rank_0_weight_3() {
        let s = rrf_score(0, 3.0);
        assert!((s - 3.0 / 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rrf_score_rank_10_weight_1() {
        let s = rrf_score(10, 1.0);
        assert!((s - 1.0 / 70.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rrf_score_large_rank_approaches_zero() {
        let s = rrf_score(1000, 1.0);
        assert!(s < 0.001);
    }

    #[test]
    fn type_affinity_same_entity() {
        assert!((type_affinity("entity", Some("entity")) - 1.2).abs() < f64::EPSILON);
    }

    #[test]
    fn type_affinity_same_concept() {
        assert!((type_affinity("concept", Some("concept")) - 1.2).abs() < f64::EPSILON);
    }

    #[test]
    fn type_affinity_same_source() {
        assert!((type_affinity("source", Some("source")) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn type_affinity_entity_concept() {
        assert!((type_affinity("entity", Some("concept")) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn type_affinity_no_preference() {
        assert!((type_affinity("entity", None) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hybrid_score_keyword_only() {
        let candidate = Candidate {
            keyword_rank: Some(1),
            vector_rank: None,
            direct_link: false,
            source_overlap: 0,
            common_neighbors: 0,
            result_type: "entity".to_string(),
        };
        let ctx = QueryContext::new(None);
        let score = hybrid_score(&candidate, &ctx);
        let expected = rrf_score(1, 1.0);
        assert!((score - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn hybrid_score_with_direct_link() {
        let candidate = Candidate {
            keyword_rank: Some(1),
            vector_rank: None,
            direct_link: true,
            source_overlap: 0,
            common_neighbors: 0,
            result_type: "entity".to_string(),
        };
        let ctx = QueryContext::new(None);
        let score = hybrid_score(&candidate, &ctx);
        let expected = (rrf_score(1, 1.0) + rrf_score(1, 3.0)) * 1.0;
        assert!((score - expected).abs() < 1e-10);
    }

    #[test]
    fn hybrid_score_all_signals() {
        let candidate = Candidate {
            keyword_rank: Some(1),
            vector_rank: Some(2),
            direct_link: true,
            source_overlap: 2,
            common_neighbors: 3,
            result_type: "entity".to_string(),
        };
        let ctx = QueryContext::new(Some("entity".to_string()));
        let score = hybrid_score(&candidate, &ctx);

        let mut expected = 0.0;
        expected += rrf_score(1, 1.0);
        expected += rrf_score(2, 1.0);
        expected += rrf_score(1, 3.0);
        expected += rrf_score(1, 4.0 * 2.0);
        expected += rrf_score(1, 1.5 * 3.0);
        expected *= 1.2;

        assert!((score - expected).abs() < 1e-10);
    }

    #[test]
    fn hybrid_score_zero_signals() {
        let candidate = Candidate {
            keyword_rank: None,
            vector_rank: None,
            direct_link: false,
            source_overlap: 0,
            common_neighbors: 0,
            result_type: "entity".to_string(),
        };
        let ctx = QueryContext::new(None);
        let score = hybrid_score(&candidate, &ctx);
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hybrid_score_source_overlap_scales() {
        let c1 = Candidate {
            keyword_rank: None,
            vector_rank: None,
            direct_link: false,
            source_overlap: 1,
            common_neighbors: 0,
            result_type: "entity".to_string(),
        };
        let c2 = Candidate {
            keyword_rank: None,
            vector_rank: None,
            direct_link: false,
            source_overlap: 3,
            common_neighbors: 0,
            result_type: "entity".to_string(),
        };
        let ctx = QueryContext::new(None);
        let s1 = hybrid_score(&c1, &ctx);
        let s2 = hybrid_score(&c2, &ctx);
        assert!(s2 > s1);
        assert!((s2 / s1 - 3.0).abs() < 1e-10);
    }

    #[test]
    fn rrf_score_monotonic_decreasing() {
        let s1 = rrf_score(1, 1.0);
        let s2 = rrf_score(2, 1.0);
        let s3 = rrf_score(10, 1.0);
        assert!(s1 > s2);
        assert!(s2 > s3);
    }

    #[tokio::test]
    async fn brain_query_empty_db_returns_empty() {
        let db = SqliteBackend::new(":memory:").await.unwrap();
        let result = brain_query(&db, None, "test query", 10, &[]).await.unwrap();
        assert_eq!(result["total"], 0);
        assert!(result["results"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn brain_query_returns_keyword_hits() {
        use crate::models::{Frontmatter, Page, PageType as PT};
        use chrono::NaiveDate;

        let db = SqliteBackend::new(":memory:").await.unwrap();
        let page = Page {
            slug: "wiki/entities/quantum".to_string(),
            vault: "forge".to_string(),
            frontmatter: Frontmatter {
                r#type: PT::Entity,
                title: "Quantum Computing".to_string(),
                tags: vec![],
                related: vec![],
                sources: vec![],
                date: NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
                status: None,
            },
            compiled_truth: "Quantum computing uses qubits.".to_string(),
            timeline: vec![],
            content_hash: "abc".to_string(),
            raw_content: "".to_string(),
        };
        db.index_page(&page).await.unwrap();

        let result = brain_query(&db, None, "quantum", 10, &[]).await.unwrap();
        assert_eq!(result["total"], 1);
        let first = &result["results"][0];
        assert_eq!(first["slug"], "wiki/entities/quantum");
        assert!(first["score"].as_f64().unwrap() > 0.0);
    }
}
