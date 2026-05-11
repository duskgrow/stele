# Quality Convention

Cross-cutting quality rules for all wiki-writing skills.

## Citations (MANDATORY)

Every fact written to a wiki page must carry an inline `[Source: ...]` citation.

Formats:
- **User's statements:** `[Source: User, {context}, YYYY-MM-DD]`
- **Meeting data:** `[Source: Meeting "{title}", YYYY-MM-DD]`
- **Web content:** `[Source: {publication}, {URL}, YYYY-MM-DD]`
- **Synthesis:** `[Source: compiled from {sources}]`
- **Conversation:** `[Source: Conversation, YYYY-MM-DD]`

### Source Precedence (highest to lowest)

1. User's direct statements (highest authority)
2. Wiki compiled truth (existing synthesized understanding)
3. Timeline entries (raw evidence)
4. External sources (web search, API enrichment)

## Back-Linking (Iron Law — MANDATORY)

Every mention of an entity/concept WITH a wiki page MUST create a backlink.

**In the new page:** Use wikilink `[[target]]` or `[[type::target]]` in the body.

**In the referenced page:** Append a timeline entry:
```json
{
  "date": "YYYY-MM-DD",
  "agent": "{agent_name}",
  "content": "Referenced in [{new page title}]({new page slug}) — {brief context}",
  "source_url": null
}
```

An unlinked mention is a broken wiki. The graph is the intelligence.

## Notability Gate

Not everything deserves a wiki page. Before creating a new page:

| Type | Gate Question |
|------|--------------|
| Entity | Will you interact with this again? Is it relevant to your work? |
| Concept | Is this a reusable mental model worth referencing later? |
| Source | Does this contain substantive information worth preserving? |
| Query | Is this a research question you'll return to? |
| Synthesis | Do you have multiple sources worth combining? |
| Comparison | Are you comparing things that matter to your work? |

**When in doubt, DON'T create.** A missing page can be created later. A junk page wastes attention and degrades search quality.

## Body Quality

- Body = **compiled truth**, not raw dump
- Synthesize, don't copy-paste
- Structure with headers, lists, tables when appropriate
- Use wikilinks to connect related pages
- Keep it concise but complete

## Timeline Quality

- Each entry should describe WHAT changed and WHY
- Include source_url when the change was triggered by external content
- Agent field identifies who made the change
- Don't duplicate information already in the body
