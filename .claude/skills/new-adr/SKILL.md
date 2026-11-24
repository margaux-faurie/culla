---
name: new-adr
description: Write a new architecture decision record in docs/adr/ (MADR 4 minimal), or supersede an existing one. Use whenever a PR makes or changes a decision about behaviour, architecture, dependencies, budgets or process.
argument-hint: "[decision title]"
---

Record the decision `$ARGUMENTS` as an ADR, following `docs/adr/0000-record-architecture-decisions.md`.

1. **Check it is new.** Read `docs/adr/README.md` and grep `docs/adr/` and `docs/SPEC.md` for the topic. If an ADR already covers it, decide whether this refines it (new ADR, old one stays accepted, note "refined by NNNN" in the index) or replaces it (supersede, step 5).
2. **Number and file.** Next number = highest `docs/adr/NNNN-*.md` + 1, four digits, never reused or renumbered. File name: `NNNN-short-kebab-title.md`, the title as a short statement of the decision.
3. **Status.** `proposed` unless Margaux made this decision: she said so in this conversation, or it is in `docs/SPEC.md` §2. Never write `accepted` for a choice an agent made. If the decision answers an open question (SPEC §10, file-engine §13), keep it `proposed` and mention it in the PR as a question for her.
4. **Write it** (20–50 lines, plain, sentence case, no exclamation marks):

   ```markdown
   ---
   status: proposed
   date: YYYY-MM-DD
   ---

   # <Decision as a statement>

   ## Context and problem statement

   <Two to five sentences: the situation and the question being decided.>

   ## Considered options

   * <Option 1>
   * <Option 2>

   ## Decision outcome

   Chosen option: "<option>", because <the main reason>.

   * <Detail, link or constraint>

   ### Consequences

   * Good, because <...>.
   * Bad, because <...>.

   Source: <links to the docs, research or benchmark that back it>.
   ```

5. **Supersede rules (ADR 0000).** An accepted ADR is not edited in substance. Write the new ADR, then change only the old one's front matter to `status: superseded by NNNN`. A PR that changes a recorded decision must include the superseding ADR.
6. **Index.** Add a row to the table in `docs/adr/README.md`: `| [NNNN](NNNN-file.md) | Title | status |`, and update the status column of any ADR it supersedes or refines.
7. **Ripple.** Update the docs that state the old decision (`docs/SPEC.md`, `docs/nfr.md`, `docs/architecture.md`, `docs/design/file-engine.md`) and `CLAUDE.md` if a command, the layout, an invariant or a convention changed. Commit with a Conventional Commit title such as `docs(adr): NNNN <title>`.
