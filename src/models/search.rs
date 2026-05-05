use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub slug: String,
    pub title: String,
    pub compiled_truth_preview: String,
    pub score: f64,
    pub signals: SearchSignals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSignals {
    pub keyword_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub direct_link: bool,
    pub source_overlap: usize,
    pub common_neighbors: usize,
    pub type_affinity: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_result_serde_roundtrip() {
        let result = SearchResult {
            slug: "wiki/entities/fns".to_string(),
            title: "FNS".to_string(),
            compiled_truth_preview: "FNS is a concept...".to_string(),
            score: 0.95,
            signals: SearchSignals {
                keyword_rank: Some(1),
                vector_rank: Some(2),
                direct_link: true,
                source_overlap: 3,
                common_neighbors: 5,
                type_affinity: 0.8,
            },
        };

        let json = serde_json::to_string(&result).unwrap();
        let restored: SearchResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.slug, restored.slug);
        assert_eq!(result.title, restored.title);
        assert_eq!(
            result.compiled_truth_preview,
            restored.compiled_truth_preview
        );
        assert!((result.score - restored.score).abs() < f64::EPSILON);
        assert_eq!(result.signals.direct_link, restored.signals.direct_link);
        assert_eq!(
            result.signals.source_overlap,
            restored.signals.source_overlap
        );
        assert_eq!(
            result.signals.common_neighbors,
            restored.signals.common_neighbors
        );
        assert!(
            (result.signals.type_affinity - restored.signals.type_affinity).abs() < f64::EPSILON
        );
    }

    #[test]
    fn search_signals_with_none_ranks() {
        let signals = SearchSignals {
            keyword_rank: None,
            vector_rank: None,
            direct_link: false,
            source_overlap: 0,
            common_neighbors: 0,
            type_affinity: 0.0,
        };

        let json = serde_json::to_string(&signals).unwrap();
        let restored: SearchSignals = serde_json::from_str(&json).unwrap();

        assert_eq!(signals.keyword_rank, restored.keyword_rank);
        assert_eq!(signals.vector_rank, restored.vector_rank);
        assert_eq!(signals.direct_link, restored.direct_link);
    }
}
