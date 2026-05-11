---
name: stele-setup
version: 1.0.0
description: |
  First-time setup for the Stele knowledge base. Initialize wiki structure,
  verify MCP connectivity, create initial pages.
author: duskgrow
tags: [stele, setup, initialization, first-run]
metadata:
  hermes:
    tags: [stele, setup, initialization]
---

# Stele Setup — First-Time Initialization

Set up the Stele knowledge base from scratch.

## When to Use

- First time using Stele
- User says "setup stele" / "初始化 wiki"
- After a fresh install or data loss

## Workflow

### Step 1: Verify Connectivity

Test MCP connection:
```yaml
tool: stats
```

If fails:
- Check MCP server is running
- Check FNS backend is running (port 9000)
- Check token at `~/.config/stele/config.toml`
- Remind user to update token if expired

### Step 2: Check Current State

```yaml
tool: page.list
params:
  dir: "."
```

If wiki is empty → proceed to Step 3.
If wiki has content → report existing structure and skip to Step 5.

### Step 3: Create Directory Structure

Create the standard directory structure by creating placeholder pages:

```yaml
tool: page.put  # for each directory
params:
  slug: "entities/.gitkeep.md"
  body: "Entity pages — people, companies, projects, tools."
  frontmatter:
    title: "Entities Directory"
    page_type: "Entity"
  timeline:
    content: "Directory initialized during setup"
```

Repeat for: `concepts/`, `sources/`, `queries/`, `synthesis/`, `comparisons/`

### Step 4: Create Index Page

```yaml
tool: page.put
params:
  slug: "index.md"
  body: |
    # Wiki Index

    ## Directories
    - [[entities/]] — People, companies, projects, tools
    - [[concepts/]] — Abstract ideas, frameworks, patterns
    - [[sources/]] — Raw material summaries
    - [[queries/]] — Research questions
    - [[synthesis/]] — Combined insights
    - [[comparisons/]] — Structured comparisons

    ## Recent Activity
    (auto-populated by stele-dream)
  frontmatter:
    title: "Wiki Index"
    page_type: "Concept"
    tags: ["index", "navigation"]
  timeline:
    content: "Index created during setup"
```

### Step 5: Sync & Verify

```yaml
tool: sync
tool: stats
```

Verify:
- All directories exist
- Index page is searchable
- Stats show expected page count

### Step 6: Report

```
Stele Setup Complete

Wiki structure: N directories, N pages
MCP endpoint: localhost:9002/mcp
Index: healthy

Next steps:
1. Start ingesting content with stele-ingest
2. Set up stele-dream cron for automated maintenance
3. Configure always_load_skills in Hermes config
