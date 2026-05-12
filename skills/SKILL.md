---
name: stele
version: 2.0.0
description: |
  Always-on wiki behavior layer. Wiki-first lookup, back-linking, source attribution,
  ambient enrichment routing. The core read/write cycle for the Stele knowledge base.
  Read this before ANY wiki interaction.
author: duskgrow
license: MIT
tags: [stele, knowledge-base, wiki, mcp, always-on, ambient]
metadata:
  hermes:
    tags: [stele, knowledge-base, wiki, mcp, always-on]
    related_skills: [hermes-agent]
---

# Stele — The Ambient Context Layer

The wiki is not an archive. It is a live context membrane that every interaction flows through in both directions.

> **Routing:** See `ROUTING.md` for skill dispatch rules (always-on + on-demand).
> **Conventions:** See `references/` for cross-cutting rules (apply to ALL operations):
>   - `references/wiki-first.md` — mandatory lookup protocol
>   - `references/quality.md` — citation, back-linking, notability rules
>   - `references/test-before-bulk.md` — batch operation safety
> **Filing:** See `references/filing-rules.md` for directory and slug conventions.

## Contract

This skill guarantees:

- Wiki is checked BEFORE any external API call (wiki-first lookup)
- Every inbound signal triggers the READ → ENRICH → WRITE loop
- Every outbound response checks wiki for relevant context
- Source attribution on every fact written (inline `[Source: ...]` citations)
- User's direct statements are highest-authority data
- Back-links maintained on every wiki write (Iron Law)

## Iron Law: Back-Linking (MANDATORY)

Every mention of an entity or concept WITH a wiki page MUST create a backlink FROM that entity's page TO the mentioning page. This is bidirectional.

**In the new page:** Use wikilink `[[target]]` or `[[type::target]]` in the body.

**In the referenced page:** Append a timeline entry:
```json
{
  "date": "YYYY-MM-DD",
  "agent": "{your_agent_name}",
  "content": "Referenced in [{new page title}]({new page slug}) — {brief context}",
  "source_url": null
}
```

An unlinked mention is a broken wiki. The graph is the intelligence.

## MCP Tool Reference (12 tools)

### Page Operations

| Tool | Purpose | Key Params |
|------|---------|------------|
| `page.get` | Read a page | `slug` (must end `.md`) |
| `page.put` | Create/update a page | `slug`, `body`, `frontmatter`, `timeline` (required) |
| `page.delete` | Delete a page | `slug` |
| `page.list` | List directory contents | `dir` (use `.` for root) |
| `page.append` | Append to page body | `slug`, `content` |

### Search & Graph

| Tool | Purpose | Key Params |
|------|---------|------------|
| `search` | FTS5 keyword search | `query`, `sort` (relevance/date/title), `type_filter`, `limit` |
| `graph.query` | Explore relationships | `slug`, `depth`, `link_type`, `direction` (in/out/both) |
| `graph.backlinks` | Find inbound links | `slug` |

### Maintenance & Management

| Tool | Purpose | Key Params |
|------|---------|------------|
| `sync` | Sync filesystem ↔ index | `dir` (optional) |
| `maintain` | Health check | `scope` (lint/orphans/backlinks/full) |
| `stats` | Wiki statistics | — |
| `reindex` | Rebuild SQLite index | — |

### Slug Rules

- Must end with `.md` suffix (e.g., `entities/fns.md`, not `entities/fns`)
- ASCII alphanumeric, `-`, `_`, `/`, `.`
- No `..` path traversal
- FNS path = slug without `.md`

### Page Types (6)

| PageType | Directory | Purpose |
|----------|-----------|---------|
| Entity | `entities/` | People, companies, projects, tools |
| Concept | `concepts/` | Abstract ideas, frameworks, patterns |
| Source | `sources/` | Raw material summaries |
| Query | `queries/` | Research questions |
| Synthesis | `synthesis/` | Combined insights |
| Comparison | `comparisons/` | Structured contrast |

### Frontmatter Schema

```yaml
title: string (required)
page_type: Entity|Concept|Source|Query|Synthesis|Comparison (required)
tags: string[] (optional)
sources: string[] (optional, external URLs)
date: string (optional, YYYY-MM-DD)
visibility: shared|private (default: shared)
created_by: string (optional, agent identifier)
```

**Forward compat:** old fields (`status`, `related`, `Stub` type) are silently ignored.

## Phases

### Phase 1: On Every Inbound — Wiki-First Lookup

Before using ANY external API to research a person, company, topic, or concept:

1. `search("keywords")` — keyword scan
2. `graph.query(slug, direction="both")` — explore neighborhood
3. `page.get(slug)` — read full compiled truth if slug found
4. `graph.backlinks(slug)` — who references this entity
5. External APIs ONLY after steps 1-4 return nothing useful

Rules:
- Steps 1-2 have results → use them, don't reach for external APIs
- User's direct statements = highest-authority data
- After any wiki write → call `sync()` so new pages are searchable

### Phase 2: On Every Write — Back-Linking + Attribution

When writing to the wiki (`page.put`, `page.append`):

1. **Back-link check:** Does the new content reference entities/concepts that have wiki pages?
   - Yes → ensure wikilinks `[[target]]` in body + timeline entry on referenced page
   - No → proceed
2. **Source attribution:** Every fact needs `[Source: ...]` inline citation
3. **Sync:** Call `sync()` after write to make new pages searchable

### Phase 3: On Every Outbound — Pull Wiki Context

Before answering any question about a topic that might be in the wiki:

1. **Check the wiki** — search for relevant pages
2. **Pull context** — use compiled truth + recent timeline
3. **Respond with context** — the wiki makes every answer better

Don't answer from general knowledge when a wiki page exists with better information.

### Phase 4: Ambient Enrichment (Default Behavior)

This is not a special mode. This is the default. Everything the user says is a potential wiki event.

- Entity mentioned → check wiki, create/enrich if needed (load `references/signal-detector.md`)
- Link shared → ingest it (load `references/ingest.md`)
- Data shared → load appropriate reference from `ROUTING.md`
- Idea expressed → capture it (load `references/signal-detector.md`)

**Rules:**
- Never interrupt the conversation to do enrichment
- Spawn sub-agents for anything that would slow down the response
- Never announce "I'm enriching the wiki" — just do it silently

## Operational Notes

- **Token expiry:** FNS token changes frequently. When MCP returns 307 "Not logged in", remind user to update token at `~/.config/stele/config.toml`. Cannot self-fix.
- **Visibility:** `visibility` and `created_by` are metadata only — no permission enforcement. Agents self-police.
- **Timeline:** append-only audit trail. Every `page.put` adds an entry. This is intentional.
- **Sources field:** external URLs only, not internal slugs. Graph relationships use wikilinks `[[target]]`.
- **page.put overwrites body:** Always `page.get` first to preserve existing content when updating.
