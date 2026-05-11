---
name: stele-ingest
version: 1.0.0
description: |
  Ingestion router. Detects input type and delegates to specialized skills.
  Does NOT do the ingestion itself — it routes.
author: duskgrow
tags: [stele, ingest, router, dispatch]
metadata:
  hermes:
    tags: [stele, ingest, router]
---

# Stele Ingest — The Router

This skill does not ingest content directly. It detects the input type and delegates to the appropriate specialized ingestion skill.

## When to Use

Trigger: user says "记录"/"摄入"/"把这条记下来"/"ingest this" or shares content worth preserving.

## Routing Logic

| Input Type | Delegate To | Description |
|-----------|-------------|-------------|
| URL / article link | `stele-page-ingest` | Fetch, analyze, compile into pages |
| PDF / document | `stele-page-ingest` | Extract content, compile into pages |
| Conversation transcript | `stele-page-ingest` | Extract entities + ideas, create pages |
| Meeting notes | `stele-page-ingest` | Extract attendees + decisions + action items |
| Book / long document | `stele-reading` | Strategic reading → structured pages |
| Video / audio | `stele-media-ingest` | Transcribe + extract + compile |
| Screenshot / image | `stele-media-ingest` | OCR + extract + compile |
| Raw text / idea | `stele-page-ingest` | Direct compilation into appropriate page type |
| User's own thought | `stele-signal` | Already handled by signal detector |

## Pre-Ingest Checklist

Before delegating, verify:

1. **Is this worth ingesting?** Not everything needs to be in the wiki.
2. **Is it already there?** Quick `search()` to check for duplicates.
3. **What's the primary subject?** Determines PageType and directory.

## Chain

```
stele-ingest (this)
  → detects type
  → delegates to stele-page-ingest / stele-media-ingest / stele-reading
  → on completion: auto sync()
```

## Post-Ingest

After any ingestion completes:
1. Call `sync()` to make new pages searchable
2. Verify wikilinks are correct (Iron Law)
3. Log what was created/updated
