---
name: stele-synthesis
version: 1.0.0
description: |
  Concept synthesis — deduplicate and merge similar wiki pages.
  Find overlapping content and create unified synthesis pages.
author: duskgrow
tags: [stele, synthesis, dedup, merge, consolidation]
metadata:
  hermes:
    tags: [stele, synthesis, dedup]
---

# Stele Synthesis — Concept Dedup + Merge

Find and resolve overlapping wiki pages. Create unified synthesis from scattered content.

## When to Use

- Multiple pages cover the same topic from different angles
- `stele-dream` Stage 8 detects overlap
- User says "这些页面有重复" / "merge these" / "synthesize"
- After bulk ingestion that may have created duplicates

## Workflow

### Phase 1: Detect Overlap

1. `search("broad topic")` — find candidate pages
2. Read each candidate: `page.get(slug)`
3. Compare content: what overlaps? what's unique?

### Phase 2: Classify Overlap

| Type | Action |
|------|--------|
| Exact duplicate | Merge into one, delete the other |
| Same topic, different angles | Create synthesis page, link from both |
| Related but distinct | Add cross-links, keep separate |
| Superset/subset | Merge subset into superset |

### Phase 3: Merge

For exact duplicates:
1. Choose the better page as base
2. Merge unique content from the other
3. Combine timelines
4. Update all pages that reference the removed page
5. Delete the removed page

### Phase 4: Synthesize

For same-topic-different-angles:
1. Create a new `synthesis/` page
2. Combine insights from all contributing pages
3. Preserve each page's unique perspective
4. Link from each contributing page to the synthesis
5. Add `[Source: compiled from {list}]` citation

### Phase 5: Update Graph

After merge/synthesis:
1. Update wikilinks in all affected pages
2. Verify back-links (Iron Law)
3. `sync()` to update index

## Anti-Patterns

- **Don't merge without reading both pages fully.**
- **Don't lose unique perspectives.** Synthesis should ADD, not REDUCE.
- **Don't auto-delete without checking references.** Other pages may link to it.
- **Don't create synthesis pages for trivial overlap.** Only when there's real value in combining.
