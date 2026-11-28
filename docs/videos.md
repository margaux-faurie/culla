# Videos: scripts and storyboards

Status: draft, 28 Nov 2026, written by an agent and not yet reviewed by Margaux. Two videos are planned: a short demo of what Culla does, and a very simple one on how to download and install it. They are published on the demo site's download and install page (decided by Margaux, 28 Nov 2026, see [Hosting](#hosting)). Lengths, bilingual captions and voice-over are proposals awaiting Margaux ([SPEC §10](SPEC.md#10-open-questions)). The scripts follow the copy rules of the app: plain, sentence case, no exclamation marks. French lines use "vous" (proposal).

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

- **Photos.** Never film the desktop app on the author's own folders, never show a real card's contents, and never show a file whose EXIF still holds GPS. In the install video, the first launch stops at the start screen with no folder picked. The demo video shows 24 of Margaux's photos (her choice, 28 Nov 2026): JPEGs already oriented, resized to 2400 px and stripped of all metadata (no EXIF, no GPS, no serial). They are swapped in for the demo's drawn photos at filming time only (`CULLA_VIDEO_PHOTOS`, see How to capture). The opening wall uses 245 of her landscape thumbnails (180×120, no people, no metadata), repeated and shuffled to fill it. Neither the photos, the thumbnails nor any frame or video made with them is committed to this repository.
- **Nothing personal on screen.** A Windows account named `demo`, no personal bookmarks, browser profile, notifications, taskbar pins, email or other windows. Turn on Do not disturb. Check every frame for user names in paths (`C:\Users\<name>`).
- **Honesty.** The demo video says on screen that the apply in the web demo is simulated. No claim appears in a video that the docs don't make: the safety lines below are taken from the README's promises.
- **Sound.** Voice-over, if any, is Margaux's own voice or a text-to-speech voice she has approved. Install video: no copyrighted music; either no music, or one CC0 or royalty-free track whose licence and source are recorded in the video's description and in this file.
- **Music of the demo video (Margaux's choice, 28 Nov 2026).** "Going Under (Instrumental Version)" by I'MIN (2:49), source: Epidemic Sound, Margaux's subscription. The master stays silent; `docs/assets/video/compose.mjs --audio <file>` lays the track under it (trimmed to the video, -16 LUFS with a two-pass `loudnorm`, 1 s fade in, 2.5 s fade out over the end card, AAC stereo, video copied). The Epidemic Sound licence is not GPL-compatible: the track and any video file with it are never committed to this repository.
- **Captions.** Install video: English and French, as separate WebVTT files (`culla-install.en.vtt`, `culla-install.fr.vtt`), plus burned-in English captions for platforms that autoplay without sound. Demo video (proposal by the agent, awaiting Margaux): no caption files; it has no voice, and every line is on screen as large text, so the on-screen text is the transcript (listed below) and the music needs no caption. Each caption stays at most 2 lines of about 42 characters, at most 15 characters per second, on screen for at least 1.5 s; a longer line from the tables below is split into consecutive captions.
- **Accessibility.** No flashing: nothing flashes more than 3 times in any second (WCAG 2.3.1), and cuts are plain cuts or short fades. On-screen text is at least 48 px high at 1080p (about 4.5 % of the frame height), with a contrast of at least 4.5:1 against what is behind it; small UI is shown zoomed in rather than at full-screen size. The cursor moves slowly and pauses before each click. A text transcript (the "voice-over" column below) is published with each video. Key presses are shown as on-screen key labels, so the video makes sense without sound.
- **Format.** 1920×1080, 30 fps, H.264 MP4 (and a WebM copy for the web), stereo AAC at -16 LUFS if there is sound. A poster frame for each video, and a short GIF cut of the demo (shots 4 and 5, under 8 s, under 5 MB) for the README (PRD S7).
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
| 32–40 | 0:16–0:20 | Review, with a punch-in on "Kept photos → Move \| Copy to D:\Photos\Selects 2026" | "Nothing moves until you say so." |
| 40–48 | 0:20–0:24 | The confirm dialog, Apply, the progress (the simulated apply runs three times faster), then a punch-in on "Apply finished. 4 of 4 photos done. Nothing was deleted." | "Nothing deleted. Ever." in orange (the one orange line); a small "Web demo: the apply is simulated" at the top right throughout (Honesty rule) |
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
3. Edit: `node docs/assets/video/compose.mjs --capture <dir> --wall <thumbnail dir> --fonts <dir> --out <dir outside the repo> [--bpm 120] [--offset 0] [--name culla-demo-30s]` (`--check` draws only the check frames). Chromium draws every frame at 1920×1080 in Bricolage Grotesque (the font files are not in the repository); ffmpeg encodes one H.264 file per scene (30 fps, `-preset veryfast`, CRF 18, no audio) and joins them with the concat demuxer and `-c copy`. Output: the video, its poster and the check frames.
   One command does steps 2 and 3 (proposal): `node docs/assets/video/render-all.mjs --photos <photo dir> --order <file,file,...> --wall <thumbnail dir> --fonts <dir> --out <dir outside the repo> [--bpm 120] [--offset 0] [--audio <track>] [--build]`. It films into `<out>/capture` and refuses an `--out` inside the repository. Run it again whenever a filmed screen changes; the review screen is due a leaner redesign in its own PR, after which the review shot is filmed again this way.
4. Music, once the track is here: measure its tempo and first downbeat, film and edit again with that BPM and offset, then `compose.mjs --master culla-demo-30s.mp4 --audio <track> --out <dir>` writes `culla-demo-30s.music.mp4` (see Music above). Never commit it.
5. Not made yet: the WebM copy and the GIF cut.

### When

M5, once the web demo has its photos, the route view, swipe, undo, review and simulated apply, and the design review against the Portfolio Design System is done. Filmed again if a later milestone changes one of these screens.

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

Decided by Margaux (28 Nov 2026, SPEC §2): the videos and the download and install guide go on the page at `culla.margauxfaurie.com`. The rest of this section is proposed by the agent and awaiting Margaux (SPEC §10, videos): the page is `culla.margauxfaurie.com/install` (`install.html` in the demo build, which renders [install.md](install.md)), and the files are self-hosted with the site (no YouTube or other embed, no third-party player or tracking: the demo makes no request outside its own assets, roadmap M5), shown with the browser's own player, never autoplayed, with the WebVTT caption files as `<track>` elements. To publish a video, add its MP4, poster and caption files to the demo's static files and fill in its entry in `src/install/status.ts`; until then the page says the videos are not recorded yet. The page refuses any source that is not a path on the site itself, and shows no player for a video without captions.

Proposed in addition, awaiting Margaux (SPEC §10): the GIF cut in the README, a link to the page from the GitHub release notes, the demo video also on the portfolio, and a video platform only if she wants one.
