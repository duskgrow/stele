use serde::{Deserialize, Serialize};

/// A directed link between two pages in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Link {
    pub source_slug: String,
    pub target_slug: String,
    pub link_type: LinkType,
    pub context_snippet: Option<String>,
}

/// The semantic type of a link between pages.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkType {
    Plain,
    Custom(String),
}

impl Serialize for LinkType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            LinkType::Plain => serializer.serialize_str("plain"),
            LinkType::Custom(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for LinkType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "plain" {
            Ok(LinkType::Plain)
        } else {
            Ok(LinkType::Custom(s))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_serde_roundtrip_plain() {
        let link = Link {
            source_slug: "page-a".to_string(),
            target_slug: "page-b".to_string(),
            link_type: LinkType::Plain,
            context_snippet: Some("see also".to_string()),
        };
        let json = serde_json::to_string(&link).expect("serialize");
        let deserialized: Link = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(link, deserialized);
    }

    #[test]
    fn link_serde_roundtrip_custom() {
        let link = Link {
            source_slug: "page-a".to_string(),
            target_slug: "page-b".to_string(),
            link_type: LinkType::Custom("cites".to_string()),
            context_snippet: None,
        };
        let json = serde_json::to_string(&link).expect("serialize");
        let deserialized: Link = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(link, deserialized);
    }

    #[test]
    fn link_serde_roundtrip_yaml_plain() {
        let link = Link {
            source_slug: "source".to_string(),
            target_slug: "target".to_string(),
            link_type: LinkType::Plain,
            context_snippet: None,
        };
        let yaml = serde_yaml::to_string(&link).expect("serialize");
        let deserialized: Link = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(link, deserialized);
    }

    #[test]
    fn link_serde_roundtrip_yaml_custom() {
        let link = Link {
            source_slug: "source".to_string(),
            target_slug: "target".to_string(),
            link_type: LinkType::Custom("references".to_string()),
            context_snippet: Some("in section 3".to_string()),
        };
        let yaml = serde_yaml::to_string(&link).expect("serialize");
        let deserialized: Link = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(link, deserialized);
    }

    #[test]
    fn link_type_plain_serializes_as_plain() {
        assert_eq!(
            serde_json::to_string(&LinkType::Plain).unwrap(),
            "\"plain\""
        );
    }

    #[test]
    fn link_type_custom_serializes_as_string() {
        assert_eq!(
            serde_json::to_string(&LinkType::Custom("cites".to_string())).unwrap(),
            "\"cites\""
        );
    }

    #[test]
    fn link_type_deserializes_plain() {
        assert_eq!(
            serde_json::from_str::<LinkType>("\"plain\"").unwrap(),
            LinkType::Plain
        );
    }

    #[test]
    fn link_type_deserializes_custom() {
        assert_eq!(
            serde_json::from_str::<LinkType>("\"cites\"").unwrap(),
            LinkType::Custom("cites".to_string())
        );
    }

    #[test]
    fn link_type_deserializes_arbitrary_string_as_custom() {
        assert_eq!(
            serde_json::from_str::<LinkType>("\"anything\"").unwrap(),
            LinkType::Custom("anything".to_string())
        );
    }
}
