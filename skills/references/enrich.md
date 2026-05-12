# Stele Enrich — Entity Enrichment Pipeline

Take an existing wiki entity page and enrich it with external data research.

## When to Use

- `signal-detector` detects an entity worth enriching
- User says "enrich this page" / "研究一下这个实体"
- `dream` triggers enrichment for stale entity pages
- After initial ingestion, entity pages need deeper content

## Enrichment Tiers

Scale effort to importance. Don't waste API calls on low-value entities.

| Tier | Who | Effort | Sources |
|------|-----|--------|---------|
| 1 (key) | Inner circle, core tools, key projects | Full pipeline | All available APIs + deep web research |
| 2 (notable) | Occasional references, industry tools | Moderate | Web search + wiki cross-reference |
| 3 (minor) | Passing mentions, one-time references | Light | Wiki cross-reference only |

## The 7-Step Protocol

### Step 1: Read Current State

```yaml
tool: page.get
params:
  slug: "<entity-slug>.md"
```

What do we already know? What's missing?

### Step 2: Wiki Cross-Reference (ALL tiers)

```yaml
tool: search
params:
  query: "<entity name>"
tool: graph.query
params:
  slug: "<entity-slug>.md"
  direction: "both"
```

The wiki is often the richest free source. Check related pages for context.

### Step 3: Extract Signal from Source

Don't just capture facts. Capture texture:

| Signal Type | What to Extract |
|-------------|----------------|
| Purpose | What it does, why it exists |
| Architecture | How it works, key components |
| Status | Active/maintained/deprecated? |
| Relationships | Dependencies, alternatives, ecosystem |
| Key people | Creators, maintainers, contributors |
| Trajectory | Where it's heading, recent changes |

### Step 4: External Research (Tier 1 + 2)

Use web search to find:
- Official documentation
- GitHub repo (stars, recent commits, issues)
- Blog posts, articles, discussions
- Social media mentions

**Key pattern:** Send existing wiki knowledge as context so search returns DELTA (what's new), not a rehash.

### Step 5: Synthesize

Combine wiki knowledge + external research into compiled truth:
- Update body with new information
- Preserve existing structure (don't rewrite from scratch)
- Add `[Source: ...]` citations for new facts

### Step 6: Update Wiki

```yaml
tool: page.put
params:
  slug: "<entity-slug>.md"
  body: "<updated compiled truth>"
  frontmatter: {tags: [...], sources: [...]}
  timeline: {content: "Enriched with {source description}"}
```

### Step 7: Back-Link Check

New information may reference other entities. Ensure:
- Wikilinks are in the body
- Referenced pages have back-link timeline entries

## When NOT to Enrich

- Entity was enriched within the past week (unless new signal)
- Entity is Tier 3 and already has reasonable content
- No new information found in research
- User hasn't interacted with this entity recently

## Anti-Patterns

- **Don't overwrite compiled truth.** Merge new info into existing structure.
- **Don't add unverified claims.** Every fact needs a source.
- **Don't over-enrich Tier 3 entities.** Light touch only.
- **Don't create duplicate pages.** Always check first.