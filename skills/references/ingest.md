# Stele Ingest — The Router

This skill does not ingest content directly. It detects the input type and delegates to the appropriate specialized ingestion skill.

## When to Use

Trigger: user says "记录"/"摄入"/"把这条记下来"/"ingest this" or shares content worth preserving.

## Routing Logic

| Input Type | Delegate To | Description |
|-----------|-------------|-------------|
| URL / article link | `page-ingest` | Fetch, analyze, compile into pages |
| PDF / document | `page-ingest` | Extract content, compile into pages |
| Conversation transcript | `page-ingest` | Extract entities + ideas, create pages |
| Meeting notes | `page-ingest` | Extract attendees + decisions + action items |
| Book / long document | `reading` | Strategic reading → structured pages |
| Video / audio | `media-ingest` | Transcribe + extract + compile |
| Screenshot / image | `media-ingest` | OCR + extract + compile |
| Raw text / idea | `page-ingest` | Direct compilation into appropriate page type |
| User's own thought | `signal-detector` | Already handled by signal detector |

## Pre-Ingest Checklist

Before delegating, verify:

1. **Is this worth ingesting?** Not everything needs to be in the wiki.
2. **Is it already there?** Quick `search()` to check for duplicates.
3. **What's the primary subject?** Determines PageType and directory.

## Chain

```
ingest (this)
  → detects type
  → delegates to page-ingest / media-ingest / reading
  → on completion: auto sync()
```

## Post-Ingest

After any ingestion completes:
1. Call `sync()` to make new pages searchable
2. Verify wikilinks are correct (Iron Law)
3. Log what was created/updated