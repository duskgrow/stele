---
name: stele-page-ingest
version: 1.0.0
description: |
  Ingest URLs, documents, conversations, and raw text into structured wiki pages.
  The core ingestion workhorse.
author: duskgrow
tags: [stele, ingest, page, compilation]
metadata:
  hermes:
    tags: [stele, ingest, page]
---

# Stele Page Ingest — Content → Wiki Pages

Compile raw materials into structured wiki pages. This is the core ingestion workhorse.

## When to Use

- User shares a URL worth preserving
- User shares a document or PDF
- User shares a conversation transcript
- User shares raw text or an idea
- `stele-ingest` routes here for page-type content

## Workflow

### Phase 1: Read Source

- **URL:** Use `web_extract` or `browser` to fetch content
- **PDF:** Extract text with appropriate tools
- **Conversation:** Read the transcript
- **Raw text:** Use directly

### Phase 2: Analyze with LLM

Identify from the source material:
- **Entities** — people, companies, tools, projects mentioned
- **Concepts** — ideas, frameworks, methodologies
- **Key facts** — the core information worth preserving
- **Sources** — URLs and references cited
- **Connections** — links to existing wiki pages

### Phase 3: Check Wiki

Before creating pages:
1. `search("entity name")` — does this already exist?
2. `page.list("entities/")` — browse existing entities
3. If exists → UPDATE (preserve timeline, add new info)
4. If not exists → CREATE (check notability gate first)

### Phase 4: Generate Pages

For each identified subject, create a page with:

1. **Correct PageType** per filing rules
2. **Complete frontmatter** (title, page_type, tags, sources, created_by)
3. **Compiled truth body** (synthesized, not copied)
4. **Wikilinks** to related pages (`[[target]]` or `[[type::target]]`)
5. **Source citations** (`[Source: ...]` inline)

### Phase 5: Write & Link

1. `page.put(slug, body, frontmatter, timeline)` for each page
2. **Back-link check:** For each wikilink in the new page, if the target has a page, append a timeline entry to the target
3. `sync()` after all writes

## Frontmatter Templates

### Entity
```yaml
title: "Entity Name"
page_type: "Entity"
tags: ["relevant", "tags"]
sources: ["https://source-url.com"]
visibility: "shared"
created_by: "{agent_name}"
date: "YYYY-MM-DD"
```

### Concept
```yaml
title: "Concept Name"
page_type: "Concept"
tags: ["relevant", "tags"]
sources: ["https://source-url.com"]
visibility: "shared"
created_by: "{agent_name}"
date: "YYYY-MM-DD"
```

### Source
```yaml
title: "Source Title"
page_type: "Source"
tags: ["relevant", "tags"]
sources: ["https://original-url.com"]
visibility: "shared"
created_by: "{agent_name}"
date: "YYYY-MM-DD"
```

## Ingest Checklist

- [ ] Checked `search` and `page.list` for existing related pages
- [ ] Chose correct `page_type` and directory prefix
- [ ] Included all required frontmatter fields (`title`, `page_type`)
- [ ] Added `sources` when content derives from external materials
- [ ] Set `created_by` to agent identifier
- [ ] Used wikilinks (`[[target]]`) to connect related pages
- [ ] Wrote body as compiled truth, not raw dump
- [ ] Added `[Source: ...]` citations for facts
- [ ] Verified back-links on referenced pages (Iron Law)
- [ ] Called `sync()` after all writes

## Anti-Patterns

- **Don't copy-paste raw content.** Synthesize and compile.
- **Don't create pages without checking for duplicates.**
- **Don't forget back-links.** Every wikilink must be bidirectional.
- **Don't skip the notability gate.** Not everything deserves a page.
