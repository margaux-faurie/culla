# Incident response

Status: draft, 27 Nov 2026. What to do when something goes wrong with a release, a key or a dependency. Threats and their mitigations are in [threat-model.md](threat-model.md); how to report a vulnerability is in [SECURITY.md](../SECURITY.md).

## Scope

- A leaked or lost updater signing key.
- A bad release: one that loses or damages photos, fails to start or fails to update.
- A security advisory in a dependency that affects Culla.
- A vulnerability reported in Culla itself.

Out of scope: problems on a user's PC that Culla did not cause, and the demo site's hosting (handled in the Cloudflare account).

## Roles

Margaux is the sole maintainer. She decides, fixes, releases and writes the notices. Agents may prepare fixes and drafts in a PR, but anything that leaves the repository (a release, an advisory, a key change) waits for her.

## Cases

### Leaked updater key

1. Stop releasing with the old key. Pull `latest.json` from the latest release so no client is offered an update in the meantime.
2. Generate a new key pair offline. Store the private key in the protected GitHub environment and in the offline backup; remove the old secret.
3. Rotate: ship one release signed with the old key that carries the new public key. Clients that update to it trust the new key from then on. Every later release is signed with the new key only.
4. If the old key may have signed something else, say so in the advisory, with the versions and hashes of the genuine releases.
5. If the old private key is lost rather than leaked, step 3 is not possible: tell users to install the next version by hand from the releases page.

### Bad release

1. Pull `latest.json` from the release so the updater stops offering it. Mark the release as not recommended in its notes.
2. Fix forward: ship a new patch version with the fix. The updater never downgrades (threat model T11b), so a rollback is a new version.
3. If photos may have been moved wrongly, the release notes explain how to find them from the apply log and the journal.

### Security advisory in a dependency

1. `cargo deny check`, Dependabot or CodeQL flags it. Check whether Culla uses the affected code path.
2. Not affected: record why in the PR or in `deny.toml` (protected path, Margaux only) with a review date.
3. Affected: update or replace the dependency, then ship a patch release and treat it as a vulnerability in Culla (below).

### Vulnerability report

1. Reports come through GitHub private vulnerability reporting ([SECURITY.md](../SECURITY.md)). Reply within 7 days.
2. Reproduce it in a private fork of the advisory, fix it there, and add a regression test.
3. Release the fix, then publish the advisory and credit the reporter unless they prefer not to be.

## Telling users

- GitHub Security Advisories for anything with a security impact, with the affected and fixed versions.
- Release notes for every case, in plain words: what happened, who is affected, what to do.
- No other channel is promised.

## After an incident

Write a short note in the PR or the advisory: what happened, how it was found, what was done, and what changes so it does not happen again. If a decision changes, write an ADR; if a threat was missing, add it to the threat model.
