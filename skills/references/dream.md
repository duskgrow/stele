# Stele Dream — The Maintenance Cycle

Scheduled maintenance cycle that keeps the wiki healthy, enriched, and evolving over time.

## When to Use

- **Scheduled:** Cron job (daily at 3am or weekly)
- **Manual:** User says "dream" / "run maintenance cycle"

## The 11 Stages

Execute in sequence. Each stage feeds the next.

### Stage 1: Sync

```yaml
tool: sync
```

Pull latest from FNS vault. Ensure index is up to date.

### Stage 2: Lint

```yaml
tool: maintain
params:
  scope: "lint"
```

Detect structural issues. Auto-fix warnings. Queue errors for review.

### Stage 3: Backlinks

```yaml
tool: maintain
params:
  scope: "backlinks"
```

Verify link integrity. Fix broken backlinks.

### Stage 4: Orphans

```yaml
tool: maintain
params:
  scope: "orphans"
```

Find orphaned pages. Evaluate:
- Worth keeping? → Create links from related pages
- Not worth keeping? → Flag for deletion (don't auto-delete)

### Stage 5: Drift Detection

For recently active pages (last 30 days of timeline entries):
1. `page.get(slug)` — read current content
2. `search("topic")` — check for new information
3. Compare: is the compiled truth still accurate?
4. If stale: flag for enrichment

### Stage 6: Extract from Recent Conversations

Review recent session transcripts for:
- Entities mentioned but not in wiki
- Ideas expressed but not captured
- Decisions made but not recorded

Create/enrich pages as needed. (Delegate to `signal-detector` logic)

### Stage 7: Pattern Discovery

```yaml
tool: graph.query
params:
  slug: "<hub-page>"
  direction: "both"
  depth: 3
```

Look for:
- Clusters of related pages that could be synthesized
- Missing links between related concepts
- Emerging themes across multiple pages

### Stage 8: Consolidation

Find pages that overlap or duplicate:
1. `search("broad topic")` — find similar pages
2. Compare content
3. Merge if appropriate (create `synthesis/` or `comparisons/` page)
4. Update links after merge

### Stage 9: Synthesis

For topics with multiple scattered pages:
1. `page.get(slug)` for each related page
2. LLM synthesize: combine insights into a coherent `synthesis/` page
3. Link from individual pages to the synthesis

### Stage 10: Auto-Think

Pick the most interesting unresolved `queries/` page:
1. Run `think` pipeline on it
2. Update the query page with new findings
3. Create new pages for discovered entities/concepts

### Stage 11: Report

Generate a maintenance report:
```
Dream Cycle Report — YYYY-MM-DD

Synced: N pages
Lint: N warnings fixed, N errors queued
Orphans: N found, N linked, N flagged
Drift: N pages flagged as stale
Extracted: N new entities, N new ideas from conversations
Patterns: N clusters found
Consolidated: N pages merged
Synthesized: N new synthesis pages
Auto-think: 1 query explored

Pages created: N
Pages updated: N
Pages flagged: N
```

## Cron Setup

```yaml
schedule: "0 3 * * *"  # Daily at 3am
prompt: |
  Run the dream maintenance cycle on the wiki.
  Execute all 11 stages. Report findings.
skills: [stele, dream, lint, think, signal-detector, enrich, synthesis]
```

## Anti-Patterns

- **Don't auto-delete pages.** Always flag for human review.
- **Don't merge pages without checking.** Some overlap is intentional.
- **Don't run all stages if wiki is small.** Skip stages 6-10 for < 50 pages.
- **Don't run during active use.** Schedule during quiet hours.