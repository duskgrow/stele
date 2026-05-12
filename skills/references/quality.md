# Quality Convention

Cross-cutting quality rules for all wiki-writing operations.

## Citations (MANDATORY)

Every fact written to a wiki page must carry an inline `[Source: ...]` citation.

- **User's statements:** `[Source: User, {context}, YYYY-MM-DD]`
- **Conversation data:** `[Source: conversation, YYYY-MM-DD]`
- **Web content:** `[Source: {publication}, {URL}, YYYY-MM-DD]`
- **Synthesis:** `[Source: compiled from {sources}]`

### Source precedence (highest to lowest)

1. User's direct statements (highest authority)
2. Compiled truth (wiki's synthesized understanding)
3. Timeline entries (raw evidence)
4. External sources (API enrichment, web search)

## Back-Linking (Iron Law, MANDATORY)

Every mention of an entity or concept WITH a wiki page MUST create a backlink
FROM that entity's page TO the mentioning page. This is bidirectional.

**In the new page:** Use wikilink `[[target]]` or `[[type::target]]` in the body.

**In the referenced page:** Append a timeline entry:
```json
{
  "date": "YYYY-MM-DD",
  "agent": "{agent_name}",
  "content": "Referenced in [{title}]({slug}) — {context}",
  "source_url": null
}
```

An unlinked mention is a broken wiki. The graph is the intelligence.

## Notability Gate

Before creating a new wiki page, check notability:

| Type | Worth creating? |
|------|----------------|
| Person | Will interact again? Relevant to work? |
| Company | Relevant to work/interests? |
| Tool/Project | Will use or reference again? |
| Concept | Reusable mental model? |
| Idea | Worth preserving the exact phrasing? |

When in doubt, DON'T create. A missing page can be created later.
A low-quality page pollutes the graph.

## Body Quality

- Compiled truth section: current best understanding, rewritten on update
- Timeline section: append-only evidence trail, never edited
- Use structured frontmatter (title, page_type, tags, sources)
- Keep pages focused — one entity or concept per page
