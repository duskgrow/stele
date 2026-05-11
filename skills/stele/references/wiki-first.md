# Wiki-First Lookup Protocol

Read this before doing ANY entity/person/company/topic lookup.

## Available Stele MCP Tools

| Tool | Use for |
|------|---------|
| `search` | Keyword search — fast, always works |
| `graph.query` | Explore page relationships (direction, link_type) |
| `graph.backlinks` | Find pages linking to a target |
| `page.get` | Direct page read when you know the slug |
| `page.list` | Browse directory contents |
| `stats` | Wiki statistics (page count, index health) |

## The 5-Step Protocol

Before using ANY external API to research a person, company, topic, or concept:

1. `search("keywords")` — keyword scan of the wiki
2. `graph.query(slug, direction="both")` — explore the neighborhood
3. `page.get(slug)` — read the full compiled truth if you found a slug
4. `graph.backlinks(slug)` — who references this entity
5. **External APIs only after steps 1-4 return nothing useful**

Never skip to external APIs without completing steps 1-4. The wiki may already have the answer.

## Rules

- **Steps 1-2 have results → use them.** Don't reach for external APIs when the wiki answered.
- **User's direct statements are highest-authority data.** The wiki captures what the user said. External sources are supplementary.
- **After any wiki page write:** call `sync()` so new pages are searchable.
- **Every wiki page reference in output** should use a clickable format: `[Page Title](slug)`

## Source Precedence (highest to lowest)

1. User's direct statements (highest authority)
2. Wiki compiled truth (existing synthesized understanding)
3. Timeline entries (raw evidence)
4. External sources (web search, API enrichment — lowest)

## When Spawning Sub-Agents

If you spawn sub-agents, include this in their task prompt:

> Follow the wiki-first lookup protocol: check stele wiki before any external API.

This ensures the convention propagates through any depth of sub-agent chain.
