# Videos: scripts and storyboards

Status: draft, 28 Nov 2026, written by an agent and not yet reviewed by Margaux. Two videos: a short demo of what Culla does, and a very simple one on how to download and install it. The demo video is 30 s (Margaux's decision, 28 Nov 2026) and has been on the install page since that day (see [Demo video published](#demo-video-published)); the install video is not recorded yet. Both go on the demo site's download and install page (decided by Margaux, 28 Nov 2026, see [Hosting](#hosting)). The install video's length, bilingual captions and voice-over are proposals awaiting Margaux ([SPEC §10](SPEC.md#10-open-questions)). The scripts follow the copy rules of the app: plain, sentence case, no exclamation marks. French lines use "vous" (proposal).

| | Demo video | Install video |
|---|---|---|
| Purpose | Show the problem and how Culla solves it, safely | Get someone from the Releases page to the start screen |
| Length | about 30 s (Margaux, 28 Nov 2026) | about 45 s |
| Filmed from | The web demo build, showing 24 of Margaux's own photos (her choice, 28 Nov 2026; see Photos) | A clean Windows 11 account installing the real release |
| How | Playwright, frame by frame, on `dist-demo/`; kinetic typography drawn and edited by script | Screen recording with OBS Studio |
| When | M5, once the demo is complete | With the first release (M6), on the release candidate |
| Published on | The demo site's install page | The demo site's install page |
| Companion doc | README, portfolio | [install.md](install.md) |

## Rules for both videos

- **Photos.** The published demo video uses the author's own photographs, chosen by her (28 Nov 2026), re-encoded so no EXIF survives, with no identifiable people; their content can still show where they were taken (threat model T17). They are JPEGs already oriented, resized to 2400 px and stripped of all metadata, swapped in for the demo's drawn photos at filming time only (`CULLA_VIDEO_PHOTOS`, see How to capture), plus about 300 of her landscape thumbnails (180×120) for the opening wall. Neither the photos, the thumbnails nor any frame made with them is committed; only the published video and its poster are (`public-demo/videos/`, Margaux, 28 Nov 2026). The demo app itself still uses placeholder scenes until its photos are chosen, then only photos with GPS and body serial stripped ([SPEC §5.5](SPEC.md#55-web-demo)). Never film the desktop app on the author's own folders, never show a real card's contents, and never show a file whose EXIF still holds GPS. In the install video, the first launch stops at the start screen with no folder picked.
- **Nothing personal on screen.** A Windows account named `demo`, no personal bookmarks, browser profile, notifications, taskbar pins, email or other windows. Turn on Do not disturb. Check every frame for user names in paths (`C:\Users\<name>`).
- **Honesty.** The demo video says on screen that the apply in the web demo is simulated. No claim appears in a video that the docs don't make: the safety lines below are taken from the README's promises.
- **Sound.** Voice-over, if any, is Margaux's own voice or a text-to-speech voice she has approved. Install video: no copyrighted music; either no music, or one CC0 or royalty-free track whose licence and source are recorded in the video's description and in this file.
- **Music of the demo video (Margaux's choice, 28 Nov 2026).** "Going Under (Instrumental Version)" by I'MIN (2:49), source: Epidemic Sound, Margaux's subscription. The master stays silent; `docs/assets/video/compose.mjs --audio <file>` lays the track under it (trimmed to the video, -16 LUFS with a two-pass `loudnorm`, 1 s fade in, 2.5 s fade out over the end card, AAC stereo, video copied). The Epidemic Sound licence is not GPL-compatible: the track and any video file with it are never committed to this repository.
- **Captions.** Install video: English and French, as separate WebVTT files (`culla-install.en.vtt`, `culla-install.fr.vtt`), plus burned-in English captions for platforms that autoplay without sound. Demo video: English and French WebVTT files that repeat the on-screen text (`public-demo/videos/culla-demo.en.vtt`, `culla-demo.fr.vtt`, off by default since the video has no voice), and the same lines as a transcript under the player (`transcript` in `src/install/status.ts`); Vitest keeps the three in step. Update them whenever the on-screen text or its timing changes. Each caption stays at most 2 lines of about 42 characters, at most 15 characters per second, on screen for at least 1.5 s; a longer line from the tables below is split into consecutive captions.
- **Accessibility.** No flashing: nothing flashes more than 3 times in any second (WCAG 2.3.1), and cuts are plain cuts or short fades. On-screen text is at least 48 px high at 1080p (about 4.5 % of the frame height), with a contrast of at least 4.5:1 against what is behind it; small UI is shown zoomed in rather than at full-screen size. The cursor moves slowly and pauses before each click. A text transcript (the "voice-over" column below) is published with each video. Key presses are shown as on-screen key labels, so the video makes sense without sound.
- **Format.** 1920×1080, 30 fps, H.264 MP4 (a WebM copy for the web is proposed; the published cut has none yet), container metadata stripped, stereo AAC at -16 LUFS if there is sound. A poster frame for each video, and a short GIF cut of the demo (shots 4 and 5, under 8 s, under 5 MB) for the README (PRD S7).
- **Look.** Title cards use the Portfolio Design System ([SPEC §5.6](SPEC.md#56-visual-design)): Night (`paper` #0c0b0a, `ink` #f1e9d8) for the demo, Bricolage Grotesque, no gradients or shadows, orange only for the one thing that matters on screen.

## (a) Demo video

About 30 s, dynamic and punchy: kinetic typography, tight UI shots and real photos (Margaux's choices, 28 Nov 2026). Story: a wall of hundreds of photos, the pain of sorting them, keep and reject at speed, the RAW following the JPEG, nothing moving before you say so, nothing deleted. The cut list is set in beats so the cuts land on the music: 60 beats, 30 s at 120 BPM; once the track's tempo and first downbeat are measured, the video is filmed and edited again at that tempo (`CULLA_VIDEO_BPM`, `--bpm`, `--offset`).

### Storyboard

Margaux's storyboard of 28 Nov 2026, with the thumbnail-wall opener she chose the same day (it replaces the hero photos and the fast run); the beat placement, the photo choices and the details marked as such are the agent's, awaiting Margaux. Times at 120 BPM.

| Beats | Time | Picture | On-screen text |
|---|---|---|---|
| 0–10 | 0:00–0:05 | A wall of photo thumbnails (Margaux's change of 28 Nov 2026): tight on a few tiles, then a fast pull-back (beats 1–4) to hundreds of photos filling the frame, with a slow drift and a slight turn. Measured: at most 1 luminance flash per second in any ninth of the frame (WCAG 2.3.1 allows 3) | "600 photos." slams in on beat 4, "One evening." on beat 6 |
| 10–16 | 0:05–0:08 | Night | "Sorting them?" then "Painful." on beat 12 |
| 16–32 | 0:08–0:16 | The swipe screen with real photos. On the beats: → keep (the card leaves right), ← reject (a dull photo leaves left), → keep, → keep. Then a quick zoom on the photo's "JPEG + NEF" label and the routes under the buttons | "Keep." (right) / "Reject." (left) / "Keep." / "Keep." as giant words, then "The RAW follows." |
| 32–40 | 0:16–0:20 | The lean review, with a punch-in on its three rows ("3 kept → Move \| Copy to D:\Photos\Selects 2026", "1 rejected → Moved to E:\2026\_rejected", "20 left in place") and the Apply button | "Nothing moves until you say so." |
| 40–48 | 0:20–0:24 | The confirm dialog, Apply, the progress (the simulated apply runs three times faster), then a light punch-in on "Apply finished. 4 of 4 photos done. Nothing was deleted." and the short route | "Nothing deleted. Ever." in orange (the one orange line); a small "Web demo: the apply is simulated" at the top right throughout (Honesty rule) |
| 48–60 | 0:24–0:30 | End card on Night | Culla wordmark, "Free. Open source. Windows 11.", `github.com/margaux-faurie/culla` |

The demo's address (`culla.margauxfaurie.com`) does not appear while the demo is behind Access. Changed from Margaux's storyboard by the agent, awaiting her: drag is not wired yet (ADR 0015), so the swipes are the arrow keys with the swipe exit animation; "Web demo: the apply is simulated" stays on the apply shot (Honesty rule), smaller than the storyboard's lines.

### How to capture

The scripts are in `docs/assets/video/` (choices marked "proposal" are the agent's, awaiting Margaux).

1. Build the demo (`pnpm build:demo`) at the commit that will be published, and note the commit in the video's description.
2. Film (`capture.spec.ts`, with its own config, kept out of the e2e run: the main config's `testDir` is `e2e/`, and Vitest reads only `src/` and `tests/`):
   `CULLA_VIDEO_CAPTURE=<dir outside the repo> CULLA_VIDEO_PHOTOS=<photo dir> CULLA_VIDEO_PHOTO_ORDER=<file,file,...> CULLA_VIDEO_BPM=120 pnpm exec playwright test -c docs/assets/video/playwright.config.ts`.
   It writes the swipe, review and apply shots as JPEG frames (quality 92) with a `timeline.json` each. Proposals, as filmed:
   - The page is 1920×1080 at device scale 1; zooms are a CSS transform drawn by the browser, so zoomed text stays sharp.
   - Frame by frame on a stopped clock (`page.clock`, CSS animations paused and stepped), because Chromium's screencast and `recordVideo` drop the frames of the 300 ms swipe exit. No trace, for the same reason.
   - Real photos: the page's drawn demo photos are swapped, as the page sets them, for the files of `CULLA_VIDEO_PHOTOS` in `CULLA_VIDEO_PHOTO_ORDER` (the session's order; `reject-` photos where a reject falls), served by Playwright from that folder. Nothing is built or committed with them.
   - The demo remembers move and its destination, as after a first apply; the swipe screen's focus outline, which runs round the whole frame, is hidden in the video.
3. Edit: `node docs/assets/video/compose.mjs --capture <dir> --wall <thumbnail dir> --out <dir outside the repo> [--fonts <dir>] [--bpm 120] [--offset 0] [--name culla-demo-30s]` (`--check` draws only the check frames). Chromium draws every frame at 1920×1080 in Bricolage Grotesque, by default the app's own (`src/assets/fonts`); ffmpeg encodes one H.264 file per scene (30 fps, `-preset veryfast`, CRF 18, no audio) and joins them with the concat demuxer and `-c copy`. Output: the video, its poster and the check frames.
   One command does steps 1 to 3 (proposal): `node docs/assets/video/render-all.mjs --photos <photo dir> --order <file,file,...> --wall <thumbnail dir> --out <dir outside the repo> [--fonts <dir>] [--bpm 120] [--offset 0] [--audio <track>] [--wall-keep-out <files>] [--wall-centre <files>] [--no-build]`. It builds the demo, films it from a server it starts itself (never one already running), edits, and ends with `node docs/assets/video/hash.mjs --write`, which certifies that the screens of the working tree it built and filmed are in the video: render from a clean tree at the commit you publish. It skips `--write` with `--no-build` or `CULLA_VIDEO_BASE_URL` (another build or server was filmed); run it by hand once sure. The wall never puts the same or a near-identical shot side by side; `--wall-keep-out` keeps some thumbnails out of the tight opening frame (Margaux: no dogs there), `--wall-centre` shows some there first. `--photos`, `--wall`, `--out` (and compose's `--capture`, `--audio`, `CULLA_VIDEO_CAPTURE`, `CULLA_VIDEO_PHOTOS`) must be outside the repository, symlinks resolved (another drive is fine), or the tools stop (`docs/assets/video/outside.mjs`); `--name` is a plain file name, and `CULLA_VIDEO_PHOTO_ORDER` only names files of the photo folder. A failed take's Playwright output goes to the system temp folder, not the repository. Run it again whenever a filmed screen changes.
4. Music, once the track is here: measure its tempo and first downbeat, film and edit again with that BPM and offset, then `compose.mjs --master culla-demo-30s.mp4 --audio <track> --out <dir>` writes `culla-demo-30s.music.mp4` (see Music above). Never commit it.
5. Publish (see Demo video published), after the render and its `hash.mjs --write`: encode the master for the web (x264 about 2.1 Mbit/s, `+faststart`), re-mux it without container metadata (`-map 0:v -c copy -map_metadata -1 -map_chapters -1 -fflags +bitexact -flags:v +bitexact`), re-save the poster without metadata, and replace `public-demo/videos/culla-demo.mp4` and `culla-demo-poster.jpg`; update the caption files and the transcript in `src/install/status.ts` when the on-screen text or its timing changes; then commit the published files with `screens.json` and `pending.md` together.
6. Not made yet: the WebM copy and the GIF cut.

### When

M5, once the web demo has its photos, the route view, swipe, undo, review and simulated apply, and the design review against the Portfolio Design System is done. Filmed again if a later milestone changes one of these screens.

### Keeping the video current

Margaux asked on 28 Nov 2026 that every change to the app is later reflected in the video. She decided on 28 Nov 2026 that it is a per-pull-request gate; the rest of the process below is proposed by the agent and awaiting her (SPEC §10, "Video freshness"). It keeps a pull request that changes a screen cheap (one line), and stops a release from shipping with a stale video.

- **What the video films.** `docs/assets/video/screens.json` lists the source paths the video shows (`paths`, with `*` within a name and `**` at any depth; tests left out by `exclude`), and `renderedFrom`, a SHA-256 over those files (each path and its bytes, sorted by path; text files read with LF line endings, fonts and images as they are) as they were at the last render, with its date in `renderedAt`. `node docs/assets/video/hash.mjs` prints the hash of the screens now (Node, no dependencies).
- **Every pull request** (a per-PR gate, decided by Margaux 28 Nov 2026). `tests/video-freshness.test.ts` (Vitest, so `pnpm test` and CI) recomputes the hash. It passes at `renderedFrom`, or at `pendingFrom`, the hash a pull request acknowledged with `node docs/assets/video/hash.mjs --pending "<date> #<PR>: <screen> changed — re-render before release"`, which adds that line to `docs/assets/video/pending.md` and records the hash. Any other hash fails, with that command in the message, so every pull request that changes a filmed screen says so; two such pull requests in parallel conflict on `pendingFrom`, and the second one, after rebasing, removes its own line and runs the command again. `--pending` refuses a line already in `pending.md`, so no pull request reuses another's. It also fails when a pattern of `screens.json` no longer matches any file, so a renamed folder cannot drop out of the list unnoticed.
- **Render-all.** Build the demo at the commit, film and edit as in How to capture, then run `node docs/assets/video/hash.mjs --write [--date YYYY-MM-DD]`: it records the new hash and date in `screens.json`, clears `pendingFrom` and empties the entries of `pending.md`. Commit both with the render's commit noted in the video's description.
- **Before a release** (proposed, awaiting Margaux). `pending.md` is empty and the video was rendered from the release commit (roadmap M6 exit criteria, testing.md §8). The `release` skill does not check this yet: adding it there is a protected-path change for Margaux.

## (b) Install video

About 45 s. Story: download, check, the SmartScreen screen, install, first launch. It follows [install.md](install.md) step by step, with the same words.

### Shot list

| # | Time | Picture | On-screen text (EN) | Voice-over (EN), proposal | Caption (FR), proposal |
|---|---|---|---|---|---|
| 1 | 0:00–0:04 | Title card (cream): Culla wordmark | Install Culla on Windows 11 | Here's how to install Culla on Windows 11. | Voici comment installer Culla sur Windows 11. |
| 2 | 0:04–0:12 | Edge on the GitHub Releases page; open Assets, click the installer and `SHA256SUMS`; the "isn't commonly downloaded" warning, then Keep and Keep anyway | Download from GitHub Releases only | Download the installer and the checksum file from the Releases page on GitHub, and only from there. | Téléchargez l'installeur et le fichier de sommes de contrôle depuis la page Releases sur GitHub, et seulement là. |
| 3 | 0:12–0:20 | PowerShell in Downloads, the `Get-FileHash` comparison printing `True`; zoom on the output | Optional: check the file | If you like, check the file in PowerShell. True means it is exactly the published file. | Si vous le souhaitez, vérifiez le fichier dans PowerShell. True signifie que c'est bien le fichier publié. |
| 4 | 0:20–0:31 | Double-click the installer; "Windows protected your PC"; slow click on More info; zoom on App and "Unknown publisher"; Run anyway | Not signed yet: More info, then Run anyway | Culla isn't code-signed yet, so Windows warns you. Check the file name, click More info, then Run anyway. | Culla n'est pas encore signé, donc Windows vous prévient. Vérifiez le nom du fichier, cliquez sur Informations complémentaires, puis Exécuter quand même. |
| 5 | 0:31–0:38 | Installer: Install, progress, Finish; no administrator prompt appears | No administrator rights needed | It installs for your account only, with no administrator rights. | L'installation se fait pour votre compte seulement, sans droits administrateur. |
| 6 | 0:38–0:45 | Culla opens on the start screen, no folder picked. End card: "Full guide: docs/install.md" with the repository address | Pick a folder and start sorting | Culla opens. Pick a folder and start sorting. Nothing moves until you confirm. | Culla s'ouvre. Choisissez un dossier et commencez le tri. Rien ne bouge avant votre confirmation. |

The French caption of shot 4 uses the labels of the French SmartScreen screen ("Informations complémentaires", "Exécuter quand même"), to check on a French Windows 11 when filming.

### How to capture

1. A clean Windows 11 machine with nothing of the author's on it: a new Hyper-V or VirtualBox virtual machine, or a new local account named `demo` on the G14. English display language for the master; display at 1920×1080 with 125 % scaling so text stays readable.
2. Download the installer through Edge, not by copying it in: SmartScreen only shows its screen for files downloaded from the internet, and that screen is the point of the video.
3. Record with OBS Studio (free, GPL) at 1920×1080, 30 fps; Windows' Snipping Tool screen recording also works. Record each shot as its own take.
4. Edit as for the demo video: zooms on the checksum output and the SmartScreen details, captions, end card.
5. Before publishing, check the video against the release: the asset name, the SmartScreen wording and the installer pages. SmartScreen can stop warning about a file once enough people have run it, and every release is a new file: if the warning no longer appears, say so in the description rather than refilming.

### When

With the first release (M6), filmed on the release candidate, then checked against the published release. Filmed again when the installer changes or when Culla becomes code-signed (shot 4 then goes, and the video gets shorter).

## Hosting

Decided by Margaux (28 Nov 2026, SPEC §2): the videos and the download and install guide go on the page at `culla.margauxfaurie.com`. The rest of this section is proposed by the agent and awaiting Margaux (SPEC §10, videos): the page is `culla.margauxfaurie.com/install` (`install.html` in the demo build, which renders [install.md](install.md)), and the files are self-hosted with the site (no YouTube or other embed, no third-party player or tracking: the demo makes no request outside its own assets, roadmap M5), shown with the browser's own player, never autoplayed, with the WebVTT caption files as `<track>` elements. To publish a video, add its MP4, poster and caption files to the demo's static files (`public-demo/videos/`) and fill in its entry in `src/install/status.ts`, with its transcript; a video without one says on the page that it is not recorded yet. The 30 s demo video is published this way; the install video is not recorded yet. The page refuses any source that is not a path on the site itself, and shows no player for a video without captions.

Proposed in addition, awaiting Margaux (SPEC §10): the GIF cut in the README, a link to the page from the GitHub release notes, the demo video also on the portfolio, and a video platform only if she wants one.

## Demo video published

Decided by Margaux (28 Nov 2026, SPEC §10): the 30 s cut (a thumbnail wall of her own photographs as the opener, no sound) is published on the install page, committed in `public-demo/videos/` with its poster, © Margaux Faurie, all rights reserved and outside the GPL (`public-demo/videos/LICENSE.txt`). The MP4 is re-muxed without container metadata and the poster re-saved without EXIF, XMP or ICC data. The page shows a transcript of its on-screen text under the player (`transcript` in `src/install/status.ts`, the same lines as `culla-demo.en.vtt`). Its line "Nothing deleted. Ever." goes beyond the README's promise; whether the next cut keeps it is open (SPEC §10). Proposed by the agent, awaiting Margaux: each new render replaces the published files under the same names; v3 was the first, replaced the same day by v4, filmed again on the lean review screen and the short route on apply (same on-screen text and timing).
