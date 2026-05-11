# Stele Query

Query the wiki knowledge base for information retrieval and synthesis.

## When to Use

This skill activates when the user asks a question that requires retrieving information from the wiki knowledge base. Any question about facts, concepts, relationships, or content stored in the wiki should use this workflow.

Trigger phrases:
- "What does the wiki say about..."
- "Find information on..."
- "Search the wiki for..."
- "Tell me about..." (when wiki content is the source)
- "What is linked to..."
- "What references..."

## Workflow

### Phase 1: Search

Start with keyword search using the `search` tool.

```yaml
tool: search
params:
  query: "<user question keywords>"
  sort: "relevance"  # or "date", "title"
```

Evaluate results. If the answer is clear from search snippets, synthesize and return. If not, proceed to Phase 2.

### Phase 2: Graph Expansion

Use `graph.query` to explore relationships between pages.

```yaml
tool: graph.query
params:
  page: "<page name from search>"
  direction: "both"  # "in", "out", or "both"
  link_type: "<optional: filter by relationship type>"
```

Direction options:
- `direction: "in"` - Find pages that reference or link TO the target page. Use this to find what references a page.
- `direction: "out"` - Find pages the target page links TO. Use this to find what a page talks about.
- `direction: "both"` - Find the full neighborhood. Use this for comprehensive context.

Use `link_type` to filter by relationship type when the user asks about specific kinds of relationships.

If backlinks are needed specifically, use `graph.backlinks`:

```yaml
tool: graph.backlinks
params:
  page: "<page name>"
```

### Phase 3: Page Retrieval

Fetch full page content with `page.get` for pages identified as relevant:

```yaml
tool: page.get
params:
  page: "<page name>"
```

### Phase 4: Synthesis

Combine information from:
- Search result snippets
- Graph relationship context
- Full page content

Format the response with:
1. Direct answer to the question
2. Supporting evidence from wiki pages
3. Source links to relevant pages
4. Related pages discovered via graph traversal

## Search Strategy

Always try strategies in this order, stopping when satisfied:

1. **Keyword search first** - Use `search` with the user's question as query
2. **Graph expansion second** - If keyword search finds a relevant page, explore its graph neighborhood with `graph.query`
3. **Full scan last** - Only if the above fail, try broader searches or ask the user to clarify

## Result Assembly

Structure responses as:

```
<Direct answer based on wiki content>

**Sources:**
- [Page Name](wiki://Page Name) - Brief description of relevance
- [Another Page](wiki://Another Page) - Brief description of relevance

**Related:**
- [Linked Page](wiki://Linked Page) - How it connects
```

## Examples

### Example 1: Direct lookup

User: "What is the Stele project?"

Tool sequence:
1. `search` with query "Stele project"
2. If results are clear, synthesize answer
3. If ambiguous, `page.get` on the most relevant result

### Example 2: Relationship query

User: "What pages reference the Architecture page?"

Tool sequence:
1. `search` with query "Architecture"
2. `graph.query` with page="Architecture", direction="in"
3. Synthesize list of referencing pages

### Example 3: Full neighborhood

User: "Tell me everything related to Authentication"

Tool sequence:
1. `search` with query "Authentication"
2. `graph.query` with page="Authentication", direction="both"
3. `page.get` on Authentication and key related pages
4. Synthesize comprehensive answer with all relationships

### Example 4: Filtered relationship

User: "What depends on the Database module?"

Tool sequence:
1. `search` with query "Database module"
2. `graph.query` with page="Database", direction="in", link_type="depends_on"
3. Synthesize list of dependencies

## Tool Reference

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `search` | Keyword search across wiki | Always start here |
| `graph.query` | Explore page relationships | After finding a relevant page |
| `graph.backlinks` | Find pages linking to a page | When you need explicit backlinks |
| `page.get` | Retrieve full page content | When snippets are insufficient |

## Parameters

### search
- `query` (required): Search terms
- `sort` (optional): "relevance", "date", or "title"

### graph.query
- `page` (required): Target page name
- `direction` (optional): "in", "out", or "both" (default: "both")
- `link_type` (optional): Filter by relationship type

### graph.backlinks
- `page` (required): Target page name

### page.get
- `page` (required): Page name to retrieve
