# Filing Rules — MANDATORY for all skills that write to the wiki

## The Rule

The **PRIMARY SUBJECT** of the content determines where it goes. Not the format, not the source, not the skill that's running.

## Decision Protocol

1. Identify the primary subject (a person? company? tool? concept? research question?)
2. File in the directory that matches the subject's PageType
3. Cross-link from related directories using wikilinks
4. When in doubt: what would you search for to find this page again? That's the slug.

## Directory → PageType Mapping

| Directory | PageType | What goes here | Example slugs |
|-----------|----------|---------------|---------------|
| `entities/` | Entity | People, companies, projects, tools, products | `entities/fns.md`, `entities/andrej-karpathy.md` |
| `concepts/` | Concept | Abstract ideas, frameworks, methodologies, patterns | `concepts/llm-wiki.md`, `concepts/rrf.md` |
| `sources/` | Source | Raw material summaries — URLs, documents, articles | `sources/2026-05-10-karpathy-gist.md` |
| `queries/` | Query | Research questions, evolving understanding | `queries/fns-alternatives.md` |
| `synthesis/` | Synthesis | Combined insights from multiple sources | `synthesis/knowledge-management-tools.md` |
| `comparisons/` | Comparison | Structured contrast between entities/concepts | `comparisons/stele-vs-gbrain.md` |

## Additional Directories (not PageType-governed)

| Directory | Purpose | Example |
|-----------|---------|---------|
| `shared/` | Multi-agent shared workspace | `shared/meeting-notes-2026-05-10.md` |
| `agents/{name}/` | Per-agent isolated workspace | `agents/hermes/task-log.md` |
| `sessions/{id}/` | Per-session records | `sessions/20260510_discussion.md` |

## Common Misfiling — DO NOT DO THESE

| Wrong | Right | Why |
|-------|-------|-----|
| Analysis of a topic → `sources/` | → appropriate subject directory | `sources/` is for raw data summaries only |
| Article about a person → `sources/` | → `entities/` | Primary subject is a person |
| Random URL → `entities/` | → `sources/` | URL content, not the entity itself |
| One-off thought → `entities/` | → `concepts/` or skip | Not everything needs a page |

## Slug Conventions

- Must end with `.md` suffix
- ASCII alphanumeric, `-`, `_`, `/`, `.`
- Lowercase preferred
- Descriptive but concise
- No `..` path traversal

Examples:
- `entities/fns.md` ✓
- `concepts/llm-wiki-methodology.md` ✓
- `sources/2026-05-10-karpathy-gist.md` ✓
- `entities/FNS.md` ✗ (use lowercase)
- `entities/fns` ✗ (missing .md)
