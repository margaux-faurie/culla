# Install Culla on Windows 11

<!-- This file is also the demo site's install page (src/install/, SPEC §2). Keep to the Markdown that reader handles: headings, paragraphs, lists, fenced code, pipe tables, inline code, bold, links, kbd. src/install/markdown.test.ts checks it. -->

Status: draft, 28 Nov 2026. There is no release yet: everything marked "(from the first release)" describes how it will work once v1.0 is out ([roadmap](roadmap.md) M6). Names of files and buttons are to be checked against the first release candidate.

Culla runs on Windows 11 only. You don't need administrator rights, an account or an internet connection to use it. Culla itself sends nothing over the internet except a daily update check, which you can turn off. To build Culla yourself instead, see [Building from source](../README.md#building-from-source).

If you would rather try Culla without installing anything, see [Try it in your browser](#try-it-in-your-browser) at the end. This guide is also on the demo site at https://culla.margauxfaurie.com/install, with a 30 s demo video; the install video comes with the first release (the site is not public yet).

## 1. Download the installer (from the first release)

1. Go to the Releases page of the project: https://github.com/margaux-faurie/culla/releases. Download Culla only from this page. A copy from anywhere else may have been changed.
2. Under the latest release, open **Assets** and download two files:
   - the installer, `Culla_<version>_x64-setup.exe` (for example `Culla_1.0.0_x64-setup.exe`; the exact name is confirmed with the first release),
   - `SHA256SUMS`, a small text file with the checksum of each file in the release.

Your browser may say that the installer "isn't commonly downloaded". Culla is new and not code-signed yet (see [step 3](#3-the-windows-protected-your-pc-screen)), so this is expected. In Microsoft Edge, open the **...** menu next to the download, choose **Keep**, then **Show more** and **Keep anyway**.

<!-- screenshot: the GitHub release page with the installer and SHA256SUMS under Assets -->
<!-- screenshot: the Edge download warning with Keep highlighted -->

## 2. Check the download (optional, from the first release)

This takes a minute and tells you that the file you have is exactly the file that was published.

1. Open the Start menu, type `PowerShell` and open **Windows PowerShell**.
2. Go to your Downloads folder:

   ```powershell
   cd $HOME\Downloads
   ```

3. Get the checksum of the installer (change the version number to match your file):

   ```powershell
   Get-FileHash .\Culla_1.0.0_x64-setup.exe -Algorithm SHA256
   ```

4. Open `SHA256SUMS` in Notepad: the file has no extension, so right-click it, choose **Open with**, then **Notepad**. Find the line that ends with the installer's name. The long code at the start of that line must be the same as the **Hash** that PowerShell printed (capital or small letters don't matter).

Or let PowerShell compare them for you. Put the installer's exact file name in `$file`; it prints `True` when they match:

```powershell
$file = "Culla_1.0.0_x64-setup.exe"
$line = Select-String -Path .\SHA256SUMS -Pattern $file -SimpleMatch | Select-Object -First 1
(Get-FileHash ".\$file" -Algorithm SHA256).Hash -eq $line.Line.Split(" ")[0]
```

If PowerShell prints an error in red instead of `True` or `False`, the file name in `$file` is not the one in your Downloads folder: check its spelling and version number, then run the three lines again.

If they don't match, don't run the installer: delete it and download it again. If they still don't match, please [open an issue](https://github.com/margaux-faurie/culla/issues).

What this check does and doesn't prove: it shows the download is complete and unchanged since it was put on the Releases page. Because the checksum file sits on the same page, it cannot prove the release itself is genuine. The next check can.

<!-- screenshot: PowerShell showing Get-FileHash output and True -->

### Check where the file was built (optional, from M6)

Once releases carry build attestations (planned for M6, not in place yet), you can check that the installer was built by the project's own GitHub workflow from its public source. This needs the GitHub CLI: run `winget install GitHub.cli`, close PowerShell and open a new PowerShell window (so it finds the `gh` command), then run `gh auth login`:

```powershell
gh attestation verify .\Culla_1.0.0_x64-setup.exe --repo margaux-faurie/culla
```

It ends with a line saying the verification succeeded. Any error means the file was not built by the project: don't run it.

## 3. The "Windows protected your PC" screen

When you open the installer, Windows 11 is likely to show a blue screen titled **Windows protected your PC**, saying that Microsoft Defender SmartScreen prevented an unrecognized app from starting.

**Why it appears.** SmartScreen warns about programs it has rarely seen, especially unsigned ones (not signed with a paid code-signing certificate). The first versions of Culla are not signed: certificates are costly, and the free route for open-source projects (SignPath Foundation) is only open to projects that have already had a release. Signing is planned after v1 ([ADR 0016](adr/0016-per-user-nsis-and-signed-updater-code-signing-deferred.md)). The warning says the file is unknown to Microsoft, not that anything bad was found in it.

**How to judge it.** Go ahead only if all of these are true:

- you downloaded the file yourself from https://github.com/margaux-faurie/culla/releases,
- the file name is the one from the release (`Culla_<version>_x64-setup.exe`),
- the checksum matched in [step 2](#2-check-the-download-optional-from-the-first-release), if you did it.

If you received the file by email, chat or from another website, don't run it.

**To continue:**

1. Click **More info**.
2. Check the line **App:** shows the installer's file name. **Publisher** shows "Unknown publisher" because the file is not signed.
3. Click **Run anyway**.

<!-- screenshot: SmartScreen first screen with "More info" highlighted -->
<!-- screenshot: SmartScreen after More info, with App, Publisher and "Run anyway" -->

**If there is no "Run anyway" button.** On some PCs, Windows 11's **Smart App Control** is on, and it blocks unsigned programs with no way to continue. We don't recommend turning it off for Culla. Use the [web demo](#try-it-in-your-browser) for now, or wait for a signed release.

## 4. Run the installer (from the first release)

The installer puts Culla in your own user folder, for your Windows account only. It does not ask for administrator rights; if a window asks you for an administrator password, stop: that is not Culla's installer.

1. Click **Install**.
2. When it finishes, click **Finish**. You can start Culla from there or from the Start menu.

Culla is added to the Start menu. Besides the app itself, keep room for its preview cache (up to 2 GB, see [Where Culla keeps its data](#where-culla-keeps-its-data)). The installer's pages and buttons are confirmed with the first release.

<!-- screenshot: the installer's Install page -->
<!-- screenshot: the installer's Finish page with "Run Culla" ticked -->

### WebView2

Culla draws its window with Microsoft Edge WebView2, a part of Windows that many apps use. It comes with Windows 11 and is kept up to date by Windows, so normally there is nothing to do.

If it is missing (for example if it was removed by hand), the installer downloads and installs it first, which needs an internet connection. If that fails, install the **Evergreen Bootstrapper** from Microsoft's WebView2 page (https://developer.microsoft.com/microsoft-edge/webview2/), then run Culla's installer again. How the first release handles a missing WebView2 is still to be confirmed ([SPEC §10](SPEC.md#10-open-questions)).

## 5. First launch

Culla opens on the start screen.

1. Choose the folder to sort, for example your SD card's `DCIM` folder. Culla only reads it at this point.
2. Choose whether the RAW (NEF) follows the JPEG, whether kept photos are moved or copied, and where they go.
3. Check the summary of where photos will go: kept photos to your destination, rejected photos to a `_rejected` folder inside the source, undecided photos left where they are. Nothing is deleted.
4. Start, and swipe: right or <kbd>→</kbd> keeps, left or <kbd>←</kbd> rejects. <kbd>?</kbd> lists every shortcut.

Nothing on your disk changes until you open the review, check the plan and confirm **Apply**.

<!-- screenshot: the start screen on first launch, with no recent folders -->

### Where Culla keeps its data

Culla keeps its own data in `%LOCALAPPDATA%\Culla` (paste this into the Explorer address bar to open it):

| Folder | What it holds |
|---|---|
| `sessions` | Your decisions and apply journals, one small database per source folder, so you can stop and resume later |
| `cache` | Previews of your photos, so they open fast next time (up to 2 GB, oldest removed first) |
| `logs` | A text report of each apply, openable from the report screen |

Nothing is ever written to your SD card or source folder except the `_rejected` folder, and only when you apply. Culla sends no telemetry. Windows also keeps WebView2's own files for Culla in `%LOCALAPPDATA%\com.margauxfaurie.culla`.

## 6. Updates (from the first release)

Culla checks for a new version once a day; you can turn this off in its settings. Each update is signed with the project's own update key, and Culla checks that signature before installing anything, so an update that was tampered with is refused. Culla never updates while an apply is in progress or waiting to be resumed.

With the check turned off, you can update by hand: download the new installer from the Releases page and run it over the old one. Your sessions and settings are kept.

## 7. Uninstall (from the first release)

1. Open **Settings**, then **Apps**, then **Installed apps**.
2. Find **Culla**, click **...** and choose **Uninstall**.
3. Follow the uninstaller's steps.

An option to also remove your Culla data (sessions, cache and logs in `%LOCALAPPDATA%\Culla`) is planned (US-43) but not built yet: the uninstaller Culla gets from Tauri does not remove that folder today. Until the option exists, the folder stays after uninstalling, so you can reinstall and resume a folder. To remove it, delete `%LOCALAPPDATA%\Culla` yourself after uninstalling.

Uninstalling never touches your photos, your `_rejected` folders or your destination folders.

<!-- screenshot: Settings, Installed apps, Culla's Uninstall menu -->
<!-- screenshot: the uninstaller with the "remove my Culla data" option, once US-43 is built -->

## Try it in your browser

The web demo at https://culla.margauxfaurie.com shows the same interface with sample photos and a simulated apply. It is not public yet: it opens to everyone when the author's portfolio does. It needs nothing installed, works in any recent browser, and sends nothing anywhere: its optional "your folder" mode reads photos in your browser only and never uploads them.

## Getting help

- Something went wrong, or a Nikon file doesn't behave: [open an issue](https://github.com/margaux-faurie/culla/issues). Attach the apply log from `%LOCALAPPDATA%\Culla\logs` if there is one, after checking it shows nothing you'd rather keep private (it contains folder and file names).
- A security problem: please report it privately, as described in [SECURITY.md](../SECURITY.md).
