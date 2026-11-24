---
status: accepted
date: 2026-11-24
---

# Conventional Commits + release-please for versioning

## Context and problem statement

Culla has one maintainer, a Rust workspace and a JS app, and ships one Windows installer. Versions must be unique, release notes must exist (OpenSSF Best Practices), and the version must stay in step across `package.json`, `Cargo.toml` and `tauri.conf.json`. How are versions and changelogs produced?

## Considered options

* Manual versioning and changelog
* Changesets
* release-please with Conventional Commits
* semantic-release

## Decision outcome

Chosen option: "release-please with Conventional Commits", because it turns commit history into a release PR with a changelog and a version bump, and the release tag triggers tauri-action.

* SemVer. Proposed addition, to confirm: `tauri.conf.json` `version` points at `../package.json`, so release-please bumps one source, plus `Cargo.toml` via `extra-files`.
* Proposed addition, to confirm: PRs are squash-merged with a Conventional Commit title. commitlint is optional.
* Changesets is built for publishing many npm packages, which does not apply here.
* Release steps and key custody: [releasing.md](../releasing.md) (planned).

### Consequences

* Good, because releases are a merge of a generated PR, with notes and a unique version.
* Good, because agents can follow one commit format, checked in CI.
* Bad, because commit titles must be disciplined; a wrong type gives a wrong version bump.
* Bad, because release-please's multi-file version bumping needs care with a mixed pnpm and Cargo workspace.

Source: [research-stack-repo.md §5](../research/research-stack-repo.md), [pre-project-docs.md](../research/pre-project-docs.md) ([OpenSSF criteria](https://www.bestpractices.dev/en/criteria/0)), [SPEC.md §7](../SPEC.md).
