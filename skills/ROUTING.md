# Stele Skill Routing Table

This file defines when to load each skill component.

## Conventions (apply to ALL operations, read first)

| Convention | Path | Scope |
|------------|------|-------|
| Wiki-first lookup | `references/wiki-first.md` | Every entity/topic lookup |
| Quality rules | `references/quality.md` | Every wiki write |
| Test before bulk | `references/test-before-bulk.md` | Every batch operation |

Sub-agents must read `references/wiki-first.md` before starting work.

## Always-On (every message)

| Skill | Reference | Action |
|-------|-----------|--------|
| signal-detector | `references/signal-detector.md` | Spawn as sub-agent. Detect entities + ideas silently. Never block main response. |
| wiki-first | `references/wiki-first.md` | Follow 5-step lookup protocol before any external API call. |

## On-Demand (load when triggered)

| Trigger | Reference | Description |
|---------|-----------|-------------|
| Shared URL / document / raw text | `references/page-ingest.md` | Compile raw materials into structured wiki pages |
| "记录" / "摄入" / shared link | `references/ingest.md` | Router — detects input type and delegates |
| Video / audio / PDF / screenshot | `references/media-ingest.md` | Media → transcription → wiki pages |
| Question about wiki content | `references/query.md` | Search + graph traversal + synthesis |
| "Who is" / "What is" / entity lookup | `references/enrich.md` | Tiered entity enrichment pipeline |
| "深度分析" / "深入研究" | `references/think.md` | Multi-round deep research pipeline |
| "dream" / cron trigger / nightly | `references/dream.md` | 11-stage maintenance cycle |
| "lint" / "health check" | `references/lint.md` | Wiki health check + maintenance |
| Merge similar concepts | `references/synthesis.md` | Concept dedup + synthesis |
| Read book / article deeply | `references/reading.md` | Strategic reading → structured pages |
| First-time setup | `references/setup.md` | Initialize wiki structure |
| Migrate from Obsidian/Notion | `references/migrate.md` | Import from other tools |

## Chain Rules

- `ingest` completes → auto `sync()` via mcp_stele_sync
- `enrich` completes → auto `sync()` via mcp_stele_sync
- `synthesis` completes → auto `sync()` via mcp_stele_sync
- `reading` completes → auto `sync()` via mcp_stele_sync
- `migrate` completes → auto `sync()` via mcp_stele_sync
- `query` returns nothing → suggest `ingest`
- `think` internally uses `query` capabilities
- `dream` internally uses `lint` + `query` + `think`
