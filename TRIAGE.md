# Triage

Stage 7. One row per request after the person has used the build: a bug, a change, or a new idea, in the person's own words. Rows are numbered T1, T2 and so on, and each commit that answers one names it ("Fix the timer reset (T4)"). `C4 Verify against hand-computed cases`.

Compare at least one result with a case worked out by hand. **Severity** is *blocks use*, *wrong*, or *wanted*. **Cost** is the agent's estimate. **Disposition** is *done*, *open*, *declined* (with the reason), or *try* (built on a branch and published as a TRY commit that says how to undo it).

| # | Request, in the person's words | Expected | What happened | Severity | Cost | Disposition |
| --- | --- | --- | --- | --- | --- | --- |
| T1 | "make the jump twice as high, make the slide jump go a little fast and jump higher, and make it more easily repeatable; i couldnt get above 1 shift jump in a row so the timing should be more lenient" | A jump twice as high; a faster, higher slide jump; slide jumps that chain landing after landing | The jump rose 0.87 m. A slide jump added 1.5 m/s and had a 50 ms window. A second slide inside a second had no boost, and every flight was shorter than that. Crouch pressed on the way down slammed from 1.5 m (R2.1) | wrong | small | done: jump 7.92 m/s (1.74 m), slide jump +2.5 along and +1.5 up, 200 ms window, the boost back after a quarter second, a slam only from 3 m (R2.2) |
