---
status: accepted
date: 2026-11-24
---

# Use one SQLite database per source for sessions and the journal

## Context and problem statement

Every decision is autosaved, a session holds thousands of events, and the apply journal must survive a crash or a pulled card. Where and how is this state stored?

## Considered options

* JSON file per folder, rewritten on each save
* JSONL append-only files (event log and journal)
* SQLite via `rusqlite`, one database per source
* One global SQLite database for all sources

## Decision outcome

Chosen option: "SQLite via `rusqlite`, one database per source", because it gives atomic, durable appends for both the event log and the journal with one dependency and one file.

* `rusqlite` with the bundled SQLite. WAL mode, `synchronous=FULL` for journal writes.
* One database per source, keyed by volume serial + relative path ([ADR 0008](0008-key-sessions-and-caches-by-volume-serial-and-relative-path.md)), stored under the local app data dir on the internal disk, never on the card.
* The same database holds the session options, groups, decision events, plans and the journal, so a journal state change and its plan are in one transaction.
* JSON rewrites risk torn files on crash. JSONL needs hand-written fsync and recovery logic for a partial last line, and two formats for two logs.
* One global database would make a corrupted file affect every source.

### Consequences

* Good, because each autosave is a small transaction, and recovery after a crash is SQLite's job.
* Good, because the journal can be queried on startup ("operations not done").
* Bad, because the bundled SQLite adds a C build to the adapters crate.
* Bad, because the format is binary; the plain-text apply log exists for humans.

Source: [research-raw-perf.md §5 (journal)](../research/research-raw-perf.md), [research-stack-repo.md, spec issue 3](../research/research-stack-repo.md), [critique.md §3–4](../research/critique.md). practices.md suggested JSONL; SPEC chose SQLite.
