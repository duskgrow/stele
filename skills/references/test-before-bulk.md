# Test Before Bulk Convention

Never run a batch operation without testing one first.

## The Process

1. **Read the skill first.** Don't write throwaway scripts. If a reference
   exists for this operation, use it.
2. **Hone the prompt/logic.** Get the output format right before running anything.
3. **Test on 3-5 items.** Run a small batch. Don't commit or push.
4. **Check the work yourself.** Read the actual output. Is quality pristine?
   Titles good? Entities extracted? Back-links created? Format clean?
5. **Fix what's wrong.** Update the reference file, not a one-off script.
   The reference is the durable artifact.
6. **Only then: bulk execute.** With throttling and a kill switch.

## Why This Matters

One bad bulk run can write 170 mediocre pages that are harder to fix than to do
right the first time. The marginal cost of testing 5 first is near zero. The cost
of cleaning up a bad bulk run is enormous.

## Applies To

- Media/video enrichment batches
- Entity enrichment batches (`enrich`)
- Wiki backfill operations
- Any cron job being deployed for the first time (`dream`)
- Any new ingestion pipeline being run at scale
- Migration batches (`migrate`)

## Anti-Patterns

- Writing a bash script from scratch instead of using an existing reference
- Running 170 items without testing 5 first
- Skipping back-link creation "as a separate step"
- Committing bulk work without reading the output
- "I'll fix the quality later"
