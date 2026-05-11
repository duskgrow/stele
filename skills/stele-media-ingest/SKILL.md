---
name: stele-media-ingest
version: 1.0.0
description: |
  Ingest video, audio, PDF, screenshot, and repo content into wiki pages.
  Handles transcription, OCR, and content extraction before compilation.
author: duskgrow
tags: [stele, ingest, media, video, audio, pdf, screenshot]
metadata:
  hermes:
    tags: [stele, ingest, media]
---

# Stele Media Ingest — Media → Wiki Pages

Ingest media content (video, audio, PDF, screenshots, repos) into structured wiki pages.

## When to Use

- User shares a YouTube/video link
- User shares a podcast/audio file
- User shares a PDF document
- User shares a screenshot/image
- User shares a GitHub repo link
- `stele-ingest` routes here for media-type content

## Content Type Handling

### Video (YouTube, etc.)
1. Fetch transcript (use `youtube-content` skill or equivalent)
2. If no transcript: summarize from description + metadata
3. Extract key points, entities, concepts
4. Compile into `sources/` page + related entity/concept pages

### Audio (podcast, recording)
1. Transcribe (use Whisper or equivalent)
2. Extract key points, entities, concepts
3. Compile into `sources/` page + related pages

### PDF / Document
1. Extract text (use `ocr-and-documents` skill or equivalent)
2. Analyze structure (headers, sections, tables)
3. Extract key content per section
4. Compile into appropriate pages

### Screenshot / Image
1. OCR or vision analysis
2. Extract text and context
3. Compile into appropriate pages

### GitHub Repo
1. Read README, key source files
2. Understand purpose, architecture, dependencies
3. Compile into `entities/` page (for the project) + related pages

## Workflow

```
Media content
  → Extract text/transcript
  → Analyze (same as stele-page-ingest Phase 2-5)
  → Generate pages
  → Write & link
  → sync()
```

## Quality Rules

- Always include the original URL in `sources` field
- For video/audio: include timestamp references when relevant
- For PDFs: include page numbers for key claims
- For repos: include the repo URL and key commit/tag if specific version matters

## Anti-Patterns

- **Don't dump full transcripts.** Synthesize.
- **Don't create pages for every video/audio.** Only if it contains substantive knowledge.
- **Don't skip transcription.** Never summarize a video you haven't seen/heard.
