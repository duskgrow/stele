---
name: stele-think
version: 1.0.0
description: |
  Multi-round deep research pipeline. Iterative search, graph traversal,
  and synthesis for complex questions that need multiple passes.
author: duskgrow
tags: [stele, think, research, deep-analysis, multi-round]
metadata:
  hermes:
    tags: [stele, think, research]
---

# Stele Think — Multi-Round Deep Research

Deep research pipeline for complex questions. Multiple rounds of search + graph + synthesis with iterative refinement.

## When to Use

- User says "深度分析" / "深入研究" / "think about"
- Complex question that can't be answered in one search pass
- Need to explore multiple angles of a topic
- Need to trace influence chains or causal paths through the knowledge graph

## How It Differs from stele-query

| | stele-query | stele-think |
|-|-------------|-------------|
| Rounds | 1 | Multiple (default 3) |
| Depth | Surface to moderate | Deep |
| Strategy | Search → graph → synthesize | Search → graph → evaluate → refine → repeat |
| Use case | "What does the wiki say about X?" | "Analyze the relationship between X, Y, and Z" |

## Workflow

### Round 1: Broad Sweep

1. `search(query)` — keyword scan for initial hits
2. `graph.query(top_result, depth=2)` — expand neighborhood
3. LLM evaluate: What do we know? What's missing? What are the key pages?

### Round 2: Targeted Deep Dive

4. `page.get(key_pages)` — read full content of most relevant pages
5. `graph.backlinks(key_pages)` — who references them
6. `search(refined_query)` — more precise search based on Round 1 findings

### Round N: Iterative Refinement

Repeat: read → search → expand → evaluate → refine query

Stop when:
- Information saturation (new rounds add nothing)
- Reached `--rounds` limit
- User is satisfied

### Final: Synthesis

Combine all rounds into structured answer:
1. Direct answer with confidence level
2. Evidence chain (which pages support which claims)
3. Information gaps (what we don't know)
4. Related questions worth exploring

## Parameters

- `--rounds N` — Maximum research rounds (default: 3)
- `--anchor SLUG` — Anchor on a specific page, explore everything connected to it
- `--focus ASPECT` — Focus on a specific aspect of the topic

## Anchor Mode

When `--anchor` is specified, the research radiates outward from a specific page:

```
Anchor page
  → direct links (depth 1)
  → indirect links (depth 2)
  → backlinks (who references this?)
  → cross-references (what else is in the same directory?)
```

This is useful for deep-diving into a single entity/concept.

## Output Format

```
## Answer
<Direct, structured answer>

## Evidence
- [Page 1](slug) — Supports claim A
- [Page 2](slug) — Supports claim B
- [Page 3](slug) — Contradicts claim A (noted)

## Gaps
- Missing information about X
- No data on Y

## Related
- [Topic Z](slug) — Worth investigating next
```

## Anti-Patterns

- **Don't run indefinitely.** Respect the rounds limit.
- **Don't re-search the same terms.** Refine queries each round.
- **Don't ignore contradictions.** Surface them explicitly.
- **Don't synthesize without evidence.** Every claim needs a page reference.
