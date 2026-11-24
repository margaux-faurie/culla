---
name: release
description: Prepare a Culla release through the release-please PR, for Margaux to merge and publish. Only Margaux starts this.
disable-model-invocation: true
---

Prepare a release. You prepare and report; Margaux merges, tags and publishes. Background: ADR 0016 (distribution and signing), ADR 0018 (Conventional Commits and release-please), `docs/releasing.md` once it exists (planned for M6), threat model T11, T11b, T11c, T20.

Never, even if a step seems to need it: merge the release PR, push a tag, create, edit or publish a GitHub Release, run a release or deploy workflow by hand, or read, print, create or rotate any signing key or secret. If a step needs one of these, stop and tell Margaux.

1. **State.** Check you are on an up-to-date `main` with a clean tree and that CI on `main` is green, including the Windows job. If not, stop and report.
2. **Release PR.** Find the open release-please PR (`gh pr list --label "autorelease: pending"`). If there is none, say so: release-please opens it from Conventional Commits on `main`; do not create one by hand.
3. **Check the PR.**
   - The version bump matches the commits since the last tag (`feat` minor, `fix` patch, `!` or `BREAKING CHANGE` major; before 1.0 follow the release-please config).
   - The changelog is readable and has no internal noise; list commits with a wrong type that skewed the bump.
   - The version is the same in `package.json`, `Cargo.toml` (workspace) and `src-tauri/tauri.conf.json`, and `Cargo.lock` is updated.
   - No open plan-breaking change without a migration: if the journal or session schema changed, recovery of open plans from the previous version is tested (T11c).
4. **Signing notes (ADR 0016).**
   - The NSIS installer is built per-user by `tauri-apps/tauri-action` on `windows-latest` as a draft release when the tag is pushed.
   - Updates are signed with the `tauri-plugin-updater` key. The private key lives only in the protected GitHub environment secret `TAURI_SIGNING_PRIVATE_KEY`, with Margaux's offline backup; the public key is in the config. Losing the private key strands installed users. Confirm the public key in the config is unchanged unless Margaux rotated it.
   - Windows Authenticode signing is deferred: v1 is unsigned, the README explains the SmartScreen warning and releases carry checksums. SignPath Foundation is applied for only after v1 is out.
5. **Report to Margaux.** The PR link, the version, a three-line summary of the changelog, every check above with pass or fail, and what she does next: merge the release PR, wait for the draft release, test the installer and the update from the previous version, then publish the draft.
