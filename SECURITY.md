# Security policy

Culla moves and renames your photo files, so security bugs can mean lost or damaged photos. Reports are welcome and are handled privately.

## Reporting a vulnerability

Report vulnerabilities privately through GitHub Security Advisories: open the repository's Security tab and select "Report a vulnerability". If the button is missing, open an issue that only asks for a private channel, with no details, and the maintainer will reply. Please do not open a public issue, pull request or discussion for a suspected vulnerability.

A useful report says what you found, the Culla version or commit, your Windows version, and the steps or file needed to reproduce it. If a crafted photo file triggers the issue, attach it to the advisory, not to a public issue.

You should get a first reply within 7 days. Culla has one maintainer, so fixes take as long as they take; you will be kept informed in the advisory. Once a fix is released, the advisory is published and you are credited unless you prefer not to be.

## Supported versions

Culla is pre-1.0. Only the latest commit on `main`, and the latest release once releases exist, receive security fixes.

## Scope

In scope:

- File operations: anything that could make Culla overwrite, remove, move or rename a file it should not, touch undecided photos, write outside the chosen folders, or follow a symlink or junction it should skip.
- Parsers and decoding: crashes, hangs, excessive memory use or memory-safety issues caused by a crafted JPEG, NEF, EXIF or sidecar file.
- Updater (once it ships): anything that could make Culla install an update that was not signed with the project's updater key.
- IPC between the user interface and the Rust backend: a way for the webview to reach files, paths or commands it should not.

Out of scope:

- Issues that need an attacker who already controls your Windows account or machine.
- The Windows SmartScreen warning on unsigned builds, which is expected until code signing is in place.
- Vulnerabilities in dependencies that do not affect Culla; report those upstream.

## Bounty

There is no bug bounty.
