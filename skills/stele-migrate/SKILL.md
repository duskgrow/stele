---
name: stele-migrate
version: 1.0.0
description: |
  Migrate knowledge from other tools (Obsidian, Notion, Logseq, plain markdown)
  into the Stele wiki. Handles format conversion, link rewriting, and page type
  inference.
author: duskgrow
tags: [stele, migrate, import, obsidian, notion, logseq]
metadata:
  hermes:
    tags: [stele, migrate, import]
---

# Stele Migrate — Import from Other Tools

Migrate knowledge from Obsidian, Notion, Logseq, or plain markdown into the Stele wiki.

## When to Use

- User says "migrate from Obsidian" / "import my notes"
- Moving from another knowledge management tool to Stele
- Bulk import of existing markdown files

## Supported Sources

| Source | Format | Notes |
|--------|--------|-------|
| Obsidian | Markdown + wikilinks | Already uses `[[links]]` — minimal conversion |
| Notion | Export (Markdown/CSV) | May need link rewriting |
| Logseq | Markdown + org-mode | Block references need conversion |
| Plain markdown | .md files | Need frontmatter + link addition |
| Roam Research | JSON export | Block references need conversion |

## Workflow

### Phase 1: Scan Source

1. Read source directory
2. Inventory: how many files, what formats, what structure
3. Report to user before proceeding

### Phase 2: Analyze & Plan

For each file:
1. Read content + existing frontmatter
2. Infer PageType from content (Entity? Concept? Source? etc.)
3. Determine target directory based on inferred type
4. Identify existing wiki pages that might conflict

Present migration plan to user for approval.

### Phase 3: Convert

For each file:
1. **Frontmatter:** Convert to Stele schema (add page_type, tags, sources, etc.)
2. **Body:** Preserve content, reformat if needed
3. **Links:** Convert `[[wikilinks]]` to Stele format (ensure `.md` suffix)
4. **Media:** Copy referenced images/files, update paths

### Phase 4: Upload

```yaml
tool: page.put
params:
  slug: "<inferred-path>.md"
  body: "<converted content>"
  frontmatter: {title, page_type, tags, sources, created_by: "migrate"}
  timeline:
    content: "Migrated from {source tool}"
    source_url: "<original path if applicable>"
```

### Phase 5: Post-Migration

1. `sync()` to update index
2. Run `stele-lint` to check for issues
3. Verify wikilinks are correct
4. Report: pages migrated, issues found, links broken

## Conflict Resolution

| Scenario | Action |
|----------|--------|
| Page already exists in wiki | Show diff, ask user: merge/overwrite/skip |
| Name collision | Add source prefix or ask user |
| Ambiguous PageType | Ask user to classify |

## Anti-Patterns

- **Don't migrate blindly.** Always show the plan first.
- **Don't lose original content.** Keep a backup of the source.
- **Don't create pages for everything.** Apply the notability gate.
- **Don't skip link verification.** Broken links are worse than no links.
