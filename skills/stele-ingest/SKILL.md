# stele-ingest

Ingest raw materials into structured wiki pages.

## When to Use

Trigger: new raw materials need compilation into wiki pages.

Examples:
- You receive a URL, document, or conversation transcript that contains knowledge worth preserving
- You complete a research task and want to save the findings
- You encounter an important entity, concept, or source that does not yet exist in the wiki
- Multiple related findings need to be linked together in a synthesis or comparison

## Workflow

1. Read raw source (URL, file, conversation)
2. Analyze with LLM: identify entities, concepts, sources, queries, synthesis opportunities, comparison candidates
3. Generate one or more pages with proper frontmatter and wikilinks
4. Write each page via `page.put`

## Directory Conventions

Pages are organized by type under these directories:

| Directory | PageType | Example slug |
|-----------|----------|-------------|
| `entities/` | Entity | `entities/fns` |
| `concepts/` | Concept | `concepts/llm-wiki` |
| `sources/` | Source | `sources/2026-05-10-karpathy-gist` |
| `queries/` | Query | `queries/fns-alternatives` |
| `synthesis/` | Synthesis | `synthesis/knowledge-management-tools` |
| `comparisons/` | Comparison | `comparisons/stele-vs-gbrain` |

Always check `page.list` for existing pages before creating new ones. Prefer updating an existing page (preserving its timeline) over creating duplicates.

## Frontmatter Templates

All pages share these fields:

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `title` | Yes | - | Human-readable page title |
| `page_type` | Yes | - | One of: Entity, Concept, Source, Query, Synthesis, Comparison |
| `tags` | No | `[]` | List of string tags |
| `sources` | No | `[]` | URLs or references to raw materials |
| `visibility` | No | `shared` | `shared` or `private` |
| `created_by` | No | - | Agent identifier that created the page |
| `date` | No | - | Creation date (YYYY-MM-DD) |

### Entity

```yaml
---
title: "FNS Vault"
page_type: "Entity"
tags: ["storage", "api"]
sources: ["https://github.com/example/fns"]
visibility: "shared"
created_by: "primary"
date: "2026-05-10"
---
```

Body: core factual information about the entity. What it is, what it does, key attributes, current status. Use wikilinks to related entities and concepts.

### Concept

```yaml
---
title: "LLM Wiki Methodology"
page_type: "Concept"
tags: ["knowledge-management", "methodology"]
sources: ["https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f"]
visibility: "shared"
created_by: "primary"
date: "2026-05-10"
---
```

Body: abstract ideas, frameworks, patterns. Explain the concept, its origins, how it works, and why it matters. Link to entities that implement or use this concept.

### Source

```yaml
---
title: "Karpathy LLM Wiki Gist"
page_type: "Source"
tags: ["reference", "llm-wiki"]
sources: ["https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f"]
visibility: "shared"
created_by: "primary"
date: "2026-05-10"
---
```

Body: summary of the raw material. Key points, quotes, and your interpretation. This is your compiled understanding of the source, not a verbatim copy.

### Query

```yaml
---
title: "What are alternatives to FNS for vault storage?"
page_type: "Query"
tags: ["research", "storage"]
sources: []
visibility: "shared"
created_by: "primary"
date: "2026-05-10"
---
```

Body: the research question, current understanding, partial answers, and what remains unknown. Update this page as you discover more. Link to sources and synthesis pages that contribute answers.

### Synthesis

```yaml
---
title: "Knowledge Management Tools Landscape"
page_type: "Synthesis"
tags: ["tools", "comparison", "km"]
sources: ["https://github.com/garrytan/gbrain", "https://gist.github.com/karpathy/..."]
visibility: "shared"
created_by: "primary"
date: "2026-05-10"
---
```

Body: combined insights from multiple sources. This is where you build new understanding by connecting dots. Explicitly reference the sources and entities you synthesized.

### Comparison

```yaml
---
title: "Stele vs gbrain"
page_type: "Comparison"
tags: ["tools", "architecture"]
sources: ["https://github.com/garrytan/gbrain"]
visibility: "shared"
created_by: "primary"
date: "2026-05-10"
---
```

Body: structured contrast between two or more entities/concepts. Use tables or sections for dimensions of comparison. Be factual and cite sources for each claim.

## Timeline Conventions

The timeline is an append-only record of how this page evolved. Each entry includes:

- `date`: when the change occurred (auto-generated on `page.put`)
- `source_url`: the URL or reference that triggered this update (null if no external source)
- `agent`: the agent identifier that made the change
- `content`: description of what changed and why

The timeline lives below the `---` divider in the markdown file. `page.put` appends new entries automatically. Do not manually edit existing timeline entries.

## page.put Examples

Create an entity page:

```json
{
  "slug": "entities/fns",
  "body": "FNS (File Name Service) is a lightweight vault server that stores markdown files and serves them over HTTP.\n\nKey features:\n- Flat namespace with directory prefixes\n- Token-based authentication\n- REST API for CRUD operations\n\n[[concepts::vault-storage]]",
  "frontmatter": {
    "title": "FNS Vault",
    "page_type": "Entity",
    "tags": ["storage", "api"],
    "sources": ["https://github.com/example/fns"],
    "visibility": "shared",
    "created_by": "primary",
    "date": "2026-05-10"
  }
}
```

Create a concept page:

```json
{
  "slug": "concepts/llm-wiki",
  "body": "The LLM Wiki methodology, proposed by Andrej Karpathy, structures knowledge into three layers:\n\n1. Raw Sources - unprocessed materials\n2. The Wiki - compiled, structured pages\n3. The Schema - rules for compilation\n\nKey principle: LLM compiles raw sources into structured truth, preserving evidence in a timeline.\n\n[[entities::karpathy]] [[sources::2026-05-10-karpathy-gist]]",
  "frontmatter": {
    "title": "LLM Wiki Methodology",
    "page_type": "Concept",
    "tags": ["knowledge-management", "methodology"],
    "sources": ["https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f"],
    "visibility": "shared",
    "created_by": "primary",
    "date": "2026-05-10"
  }
}
```

Update an existing page (new content merges with existing timeline):

```json
{
  "slug": "entities/fns",
  "body": "Updated content here...",
  "frontmatter": {
    "title": "FNS Vault",
    "page_type": "Entity",
    "tags": ["storage", "api", "updated"]
  }
}
```

## Ingest Checklist

- [ ] Checked `page.list` and `search` for existing related pages
- [ ] Chose correct `page_type` and directory prefix
- [ ] Included all required frontmatter fields (`title`, `page_type`)
- [ ] Added `sources` when content derives from external materials
- [ ] Set `created_by` to your agent identifier
- [ ] Used wikilinks (`[[target]]` or `[[type::target]]`) to connect related pages
- [ ] Verified `visibility` (default `shared` unless sensitive)
- [ ] Wrote body as compiled truth, not raw dump
- [ ] Timeline will be auto-populated by `page.put`

## Related Skills

- `stele-query` - search and retrieve pages from the wiki
- `stele-lint` - run health checks on the wiki structure
