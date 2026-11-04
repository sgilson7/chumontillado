# Journal entries

The design journal is where the person reads the build. Each entry reports one thing that happened, cites the rows that record it, and says what is still the person's to judge. [`docs/journal.md`](https://github.com/sgilson7/builder-setting/blob/main/docs/journal.md) says why the journal exists and what it cannot show.

## Rules

1. The journal starts with the opening entry (kind *reading*), written before `PLAN.md`. Every project has one unless `BRIEF.md`'s Journal section says `none`.
2. Every decision put to the person, every visible change, every divergence from the plan and every deploy gets an entry, written when it happens.
3. Entries are newest first on their page, and never removed. A correction is a new entry, or a note added to the old one ("replaced by …, T7").
4. An entry cites the rows it reports: `T4` for a triage row, `R2.3` for a notebook row, `XX-Q5` for a question. Every *person* row in `SECOND-ORDER.md` is named somewhere in the journal.
5. Pictures are of the running build, or drawn from the same numbers the code uses. A picture made separately from the build can disagree with it.
6. Each entry says what still looks wrong and what has not been done ("Nothing is pushed").
7. An entry written after the fact has the kind *retrospective* and names its sources.

`scripts/journal_check.py` checks the form of rules 1, 3 and 4. Whether an entry is right is the person's reading.

## Kinds

The kind goes in the stamp and in `data-kind`. The label is the AI-literacy step the entry asks the reader to take, the same labels `docs/procedure.md` gives the stages.

| Kind | Stage | Label | Holds |
| --- | --- | --- | --- |
| reading | 0 | B1, C3 | what Builder asks of the agent, in its words; what the project adds or changes; facts measured before any edit; numbered rules, each with what holds it; the ID prefix; where the agent stops; questions, each with a recommendation and what it blocks |
| agreed | 0 | C5 | each question, the person's answer, and what it changes; which rules moved into `CLAUDE.md` and which became tests |
| brief | 1 | C1, A1 | the description word for word; a table from each requirement to where it lands, with question numbers |
| plan | 2 | B2 | the build order; diagrams drawn from the numbers the code will use; files touched and why |
| mockups | 2 | B2 | what the tool will look like, drawn before it is built |
| for you | 3 | A0, B2 | each decision as `XX-Qn`: the default the agent builds on, the alternative, and `changes:` naming what an override costs. An answer that differs becomes a TRIAGE row citing the question |
| measured | 4 | B2 | the baseline check, step by step |
| screenshots | 5 | C4 | pictures of the running build with one-line captions, and the weak spots |
| divergences | 6 | A3, B3 | the plan, what happened, what changed, citing notebook rows |
| ready for you | 7 | C4 | how to open the build, what to try first, rough edges, checks with counts |
| your fixes | 7 | C4, B4 | the triage row, what the person said, what changed, with pictures |
| live | 8 | C4 | where it was published, the check against the live address, what is left and whose it is |
| review | any | A1–A6 | the person or a second model checks an entry against the build and says what it could not verify |
| retrospective | any | C5 | an entry written after the fact, with its sources |

## Markup

```html
<article class="entry" id="2026-10-19-plan-route" data-kind="plan">
  <div class="stamp"><time datetime="2026-10-19">2026-10-19</time><span>plan</span></div>
  <div class="entry-body">
    <h3>A plain sentence about what happened</h3>
    <p>The lede: what changed, what still looks wrong.</p>
    <div class="grid">
      <figure><button aria-label="Open full-window"><img src="img/route/plan.svg" alt="The route over the floor plan"></button>
        <figcaption><b>The route.</b> Drawn from the numbers in layout.rs.</figcaption></figure>
    </div>
  </div>
</article>
```

A *for you* entry's questions:

```html
<ol class="qs">
  <li><strong>XX-Q3. The question.</strong>Default: what I build on, and why. Alternative: the other choice.<em>changes: what an override costs</em></li>
</ol>
```

Give each entry an `id` that will not change (the date and a word or two), add its row to the home page's timeline, and update the page's card. For a decision that waits on the person, use `<span class="you">for you</span>` in the stamp.

## Pictures

Capture the running build headless (the browser check's Playwright works) at the size the person will view it, frame each shot on what changed, look at every shot before using it, and retake any that reads badly. Save them under the journal's `img/<page>/<topic>/`. A game with its own camera tooling uses it; `kits/canvas/` has a tile script. Never open a visible window on the person's screen.

## Publishing

The journal is a static page that works opened from disk. An agent that can publish it to a private page (a claude.ai artifact, for example) may do so and keep one address for its life, unless the brief says the content is confidential. Commit in the journal's own repository after every change.
