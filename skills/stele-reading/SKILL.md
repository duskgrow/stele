---
name: stele-reading
version: 1.0.0
description: |
  Strategic reading — read a book, article, or case study through the lens
  of a specific problem, then compile structured pages from the insights.
author: duskgrow
tags: [stele, reading, book, article, strategic, compilation]
metadata:
  hermes:
    tags: [stele, reading, book]
---

# Stele Reading — Strategic Reading → Wiki Pages

Read a book, article, or case study through the lens of a specific problem. Produce structured wiki pages from the insights.

## When to Use

- User says "读这本书" / "read this" / "analyze this article in depth"
- Long-form content that needs systematic extraction
- Content where the user wants to extract actionable insights, not just facts

## How It Differs from stele-page-ingest

| | stele-page-ingest | stele-reading |
|-|-------------------|---------------|
| Depth | Surface compilation | Deep strategic analysis |
| Input | URL, doc, conversation | Book, long article, case study |
| Output | Facts + entities | Insights + frameworks + playbooks |
| Time | Minutes | Hours/days (multi-session) |

## Workflow

### Phase 1: Define the Lens

Before reading, establish:
- **What problem are you trying to solve?**
- **What would be valuable to extract?**
- **How does this connect to existing wiki knowledge?**

### Phase 2: Read & Extract

For books: read chapter by chapter
For articles: read section by section
For case studies: read chronologically

Extract per section:
- Key claims and arguments
- Evidence and examples
- Frameworks and mental models
- Actionable insights
- Connections to existing knowledge

### Phase 3: Compile Pages

Create multiple pages from the reading:

| Page Type | Content | Directory |
|-----------|---------|-----------|
| Source | Summary of the material itself | `sources/` |
| Entity | People/companies/projects mentioned | `entities/` |
| Concept | Frameworks, mental models, methodologies | `concepts/` |
| Synthesis | Cross-chapter insights, connections | `synthesis/` |
| Query | Questions raised, unresolved issues | `queries/` |

### Phase 4: Cross-Link

Ensure all pages reference each other:
- Source page links to all extracted entities/concepts
- Concept pages link back to source
- Synthesis pages link to all contributing pages

### Phase 5: Strategic Summary

Create a final `synthesis/` page that answers:
- What did you learn that's actionable?
- How does this change your understanding?
- What should you do differently based on this?

## Output

Multiple wiki pages + one strategic synthesis page.

## Anti-Patterns

- **Don't just summarize chapters.** Extract insights.
- **Don't create a page for every fact.** Group related facts into concepts.
- **Don't skip the strategic lens.** Reading without purpose creates noise.
- **Don't forget the "so what?"** Every page should answer why this matters.
