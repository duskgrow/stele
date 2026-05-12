# Signal Detector — Ambient Wiki Capture

Lightweight sub-agent that fires on every inbound message to capture TWO things with EQUAL priority:

1. **Original thinking** — the user's ideas, observations, theses, frameworks
2. **Entity mentions** — people, companies, projects, tools, concepts

Original thinking is AT LEAST as valuable as entity extraction. Ideas are the intellectual capital. Entities are bookkeeping. Both compound over time.

## Contract

This skill guarantees:
- Fires on every message (no exceptions unless purely operational)
- Runs in parallel (spawned sub-agent, never blocks main response)
- Captures ideas with the user's EXACT phrasing (no paraphrasing)
- Detects entity mentions and creates/enriches wiki pages
- Logs a one-line summary of what was captured

## Phases

### Phase 1: Entity Detection

Extract entity mentions from the message:

1. Identify named entities: people, companies, projects, tools, concepts
2. For each entity: `search("entity name")` — does a page already exist?
   - **If yes:** Enrich — append timeline entry with new signal
   - **If no:** Check notability gate → create page or skip
3. Use correct PageType and directory per filing rules

### Phase 2: Idea Capture

Detect original thinking in the message:

1. Look for: opinions, observations, theses, frameworks, hypotheses, predictions
2. Check if similar idea already exists in wiki (`search("idea keywords")`)
3. If new: create a page (usually `concepts/` or `queries/`) with user's EXACT phrasing
4. If existing: append new perspective to timeline

**Critical:** Capture the user's exact words. Do not paraphrase. Do not editorialize.

### Phase 3: Signal Logging

Always log a one-line summary (visible in agent logs, not to user):

```
Signals: 0 ideas, 0 entities, 0 facts (skipped: operational)
Signals: 1 idea (captured → concepts/llm-wiki-methodology), 2 entities (enriched → entities/fns, entities/hermes)
```

This makes the ambient capture loop debuggable.

## Output Format

No visible output to the user. This skill runs silently in the background.
The output is wiki pages created/updated and the signal log line.

## Anti-Patterns

- **Don't create stub pages.** A page with just a title and no useful content is noise.
- **Don't paraphrase ideas.** The user's exact phrasing IS the value.
- **Don't interrupt the conversation.** This runs in the background.
- **Don't over-detect.** Not every noun is an entity worth a page.
- **Don't create pages for transient references.** Only create if the entity is likely to be referenced again.

## Notability Gate (Quick Check)

| Type | Worth creating? |
|------|----------------|
| Person | Will interact again? Relevant to work? |
| Company | Relevant to work/interests? |
| Tool/Project | Will use or reference again? |
| Concept | Reusable mental model? |
| Idea | Worth preserving the exact phrasing? |

When in doubt → skip. Missing pages can be created later.