# Wiki-First Lookup Convention

**Read this before doing ANY entity/person/company/topic lookup.**

Sub-agents and fresh sessions inherit Stele MCP tools but not the knowledge
of when and how to use them. This file is that knowledge.

## Available Stele MCP Tools

| Tool | Use for |
|------|---------|
| `mcp_stele_search` | FTS5 keyword search — fast, always works |
| `mcp_stele_page_get` | Direct page read when you know the slug |
| `mcp_stele_graph_query` | Explore relationships (in/out/both) |
| `mcp_stele_graph_backlinks` | Who references this entity |
| `mcp_stele_page_list` | List directory contents |
| `mcp_stele_stats` | Wiki statistics |
| `mcp_stele_page_put` | Create or update a page |
| `mcp_stele_page_delete` | Delete a page |
| `mcp_stele_sync` | Sync filesystem ↔ index |
| `mcp_stele_maintain` | Health check (lint/orphans/backlinks/full) |
| `mcp_stele_reindex` | Rebuild SQLite index |

## The Lookup Chain (MANDATORY ORDER)

1. **`search("keywords")`** — keyword scan, fast, zero API cost
2. **`graph.query(slug, direction="both")`** — explore neighborhood if slug found
3. **`page.get(slug)`** — read full compiled truth if slug found
4. **`graph.backlinks(slug)`** — who references this entity
5. **External APIs ONLY after steps 1-4 return nothing useful**

Never skip to external APIs without completing steps 1-4. The wiki has
knowledge accumulated over many sessions. The answer is almost always there.

## Rules

- **Wiki results exist → use them.** Don't reach for external APIs when the wiki answered.
- **User's direct statements are highest-authority data.** The wiki captures
  what the user said in conversations and notes. External sources are supplementary.
- **After any wiki write:** call `sync()` so new pages are searchable.
- **Every wiki page reference in output** should use the slug format for linking.
- **Don't use Hindsight recall for entity lookups.** Hindsight stores conversation
  memory, not the wiki knowledge graph. Use `search` or `graph.query` for entity lookups.

## When Spawning Sub-Agents

If you spawn sub-agents, include this line in their task prompt:

> Read `references/wiki-first.md` before starting work.

This ensures the convention propagates through any depth of sub-agent chain.
