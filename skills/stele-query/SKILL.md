---
name: stele-query
version: 1.0.0
description: |
  Search, graph traversal, and synthesis from the wiki knowledge base.
  The primary retrieval skill.
author: duskgrow
tags: [stele, query, search, graph, retrieval]
metadata:
  hermes:
    tags: [stele, query, search, graph]
---

# Stele Query — Search + Graph + Synthesis

Retrieve information from the wiki knowledge base. The primary retrieval skill.

## When to Use

- User asks a question that might be answered by wiki content
- User says "wiki 里有没有..." / "关于 X 你查一下"
- Any question about facts, concepts, relationships stored in the wiki
- Before answering from general knowledge, check the wiki first

## Workflow

### Phase 1: Keyword Search

```yaml
tool: search
params:
  query: "<user question keywords>"
  sort: "relevance"  # or "date", "title"
```

Evaluate results. If the answer is clear from search snippets, synthesize and return. If not, proceed to Phase 2.

### Phase 2: Graph Expansion

```yaml
tool: graph.query
params:
  slug: "<page slug from search>"
  direction: "both"  # "in", "out", or "both"
  depth: 2
```

Direction options:
- `in` — Find pages that reference this page
- `out` — Find pages this page references
- `both` — Full neighborhood

### Phase 3: Page Retrieval

For pages identified as relevant:

```yaml
tool: page.get
params:
  slug: "<page-slug>.md"
```

### Phase 4: Synthesis

Combine information from:
- Search result snippets
- Graph relationship context
- Full page content
- Timeline entries (for recent changes)

Format the response:
1. Direct answer to the question
2. Supporting evidence from wiki pages
3. Source links to relevant pages
4. Related pages discovered via graph traversal

## Search Strategy

Try in order, stopping when satisfied:

1. **Keyword search** — `search()` with user's question
2. **Graph expansion** — `graph.query()` on the most relevant result
3. **Backlinks** — `graph.backlinks()` to find what references the topic
4. **Broader search** — wider terms or ask user to clarify

## Result Format

```
<Direct answer based on wiki content>

Sources:
- [Page Title](slug) — Brief description of relevance
- [Another Page](slug) — Brief description

Related:
- [Linked Page](slug) — How it connects
```

## Tool Reference

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `search` | Keyword search | Always start here |
| `graph.query` | Explore relationships | After finding a relevant page |
| `graph.backlinks` | Find inbound links | When you need what references a page |
| `page.get` | Read full page | When snippets are insufficient |
| `page.list` | Browse directory | When exploring what exists |

## Parameters

### search
- `query` (required): Search terms
- `sort` (optional): "relevance", "date", "title"
- `type_filter` (optional): Filter by PageType
- `limit` (optional): Max results

### graph.query
- `slug` (required): Target page slug
- `direction` (optional): "in", "out", "both" (default: "both")
- `depth` (optional): How many hops (default: 2)
- `link_type` (optional): Filter by relationship type

### graph.backlinks
- `slug` (required): Target page slug

### page.get
- `slug` (required): Page slug (must end `.md`)
