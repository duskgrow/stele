# Stele Lint — Health Check + Maintenance

Periodic health check for the wiki knowledge base.

## When to Use

- **Manual:** User says "lint", "health check", "maintenance check"
- **Scheduled:** Cron job (daily/weekly)
- **After bulk ingestion:** Verify integrity after batch writes

## Workflow

### Step 1: Structure Check

```yaml
tool: maintain
params:
  scope: "lint"
```

Checks:
- Empty titles
- Invalid slugs
- Empty bodies
- Source-type pages without sources
- Pages without tags
- Pages without timeline entries

### Step 2: Orphan Detection

```yaml
tool: maintain
params:
  scope: "orphans"
```

Finds pages with zero inbound links. These are disconnected from the knowledge graph.

### Step 3: Link Integrity

```yaml
tool: maintain
params:
  scope: "backlinks"
```

Verifies that all wikilinks have corresponding backlinks (Iron Law compliance).

### Step 4: Full Check

```yaml
tool: maintain
params:
  scope: "full"
```

Runs all checks in sequence.

### Step 5: LLM Analysis

Review findings and:
1. Prioritize by severity
2. Suggest fixes for each issue
3. Auto-fix warnings where the fix is unambiguous
4. Queue errors for human review

## Severity Levels

| Severity | Action | Examples |
|----------|--------|----------|
| **error** | Requires human confirmation | Empty title, invalid slug, broken link target |
| **warning** | Auto-fix unless contradicted | Empty sources on Source page, empty tags, empty timeline |

## Output Format

```
[SEVERITY] Page: <slug> - Issue description
  Fix: <suggested action>
  Context: <why this matters>
```

Group by severity. Sort by page slug within each group.

## Auto-Fix Rules

| Issue | Auto-Fix | Condition |
|-------|----------|-----------|
| Empty tags | Suggest tags from content analysis | Only if content is clear |
| Empty sources on Source page | Flag for review | Can't auto-fix — need actual source |
| Empty timeline | Add initial entry | Only if page has content |
| Orphan page | Flag for review | Can't auto-delete — might be intentional |

## Cron Schedule

Recommended: weekly health check

```
Schedule: 0 3 * * 0  (Sunday 3am)
Prompt: "Run lint on the wiki. Report findings and auto-fix warnings."
Skills: [stele, lint]
```