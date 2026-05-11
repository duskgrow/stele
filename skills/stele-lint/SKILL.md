# stele-lint

Periodic health check and maintenance skill for the Stele knowledge base.

## Trigger

- **Automatic**: Cron-scheduled periodic health check (daily/weekly)
- **Manual**: User requests "run lint", "health check", "maintenance check", or "stele-lint"

## Workflow

Run in sequence:

1. **maintain(lint)** - Detect structural and content issues
2. **maintain(orphans)** - Find orphaned pages with no inbound links
3. **maintain(backlinks)** - Verify and repair backlink integrity
4. **LLM analysis** - Review findings, prioritize, suggest fixes

## Severity Levels

| Severity | Action | Examples |
|----------|--------|----------|
| **error** | Requires human confirmation before fix | Empty title, invalid slug |
| **warning** | Auto-fix unless contradicted by context | Empty sources, empty tags, empty timeline, empty compiled_truth |

Auto-fix warnings when the fix is unambiguous. Flag errors for review.

## Output Format

```
[SEVERITY] Page: <slug> - Issue description
  Fix suggestion: <action>
  Context: <why this matters>
```

Group by severity. Sort by page slug within each group.

## Lint Checks

### Errors (human confirmation required)

| Check | Description | Fix Suggestion |
|-------|-------------|----------------|
| Empty title | Page has no title field | Prompt user for title or mark for deletion |
| Invalid slug | Slug does not match URL-safe format (`/^[a-z0-9-]+$/`) | Suggest valid slug derived from title |

### Warnings (auto-fix eligible)

| Check | Description | Fix Suggestion |
|-------|-------------|----------------|
| Empty sources | Source-type page has empty sources array | Remove Source designation or add sources |
| Empty tags | Page has zero tags | Suggest tags from content analysis |
| Empty timeline | Page has empty timeline | Remove timeline field or add events |
| Empty compiled_truth | Page has empty compiled_truth | Flag for content review or remove field |

## Execution

1. Run `maintain lint` to collect all issues
2. Run `maintain orphans` to find unlinked pages
3. Run `maintain backlinks` to verify graph integrity
4. Present findings in the output format above
5. Apply auto-fixes for warnings with high confidence
6. Queue errors for human review with suggested fixes

## Dependencies

- `maintain` tool with scopes: `lint`, `orphans`, `backlinks`, `full`
- Access to page metadata: title, slug, tags, timeline, sources, compiled_truth
