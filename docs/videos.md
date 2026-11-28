# Videos: scripts and storyboards

Status: draft, 28 Nov 2026, written by an agent and not yet reviewed by Margaux. Two videos are planned: a short demo of what Culla does, and a very simple one on how to download and install it. They are published on the demo site's download and install page (decided by Margaux, 28 Nov 2026, see [Hosting](#hosting)). Lengths, bilingual captions and voice-over are proposals awaiting Margaux ([SPEC §10](SPEC.md#10-open-questions)). The scripts follow the copy rules of the app: plain, sentence case, no exclamation marks. French lines use "vous" (proposal).

| | Demo video | Install video |
|---|---|---|
| Purpose | Show the problem and how Culla solves it, safely | Get someone from the Releases page to the start screen |
| Length | 75–85 s (target 80 s) | about 45 s |
| Filmed from | The web demo build, with the demo's own photos | A clean Windows 11 account installing the real release |
| How | Playwright video on `dist-demo/`, edited after | Screen recording with OBS Studio |
| When | M5, once the demo is complete | With the first release (M6), on the release candidate |
| Published on | The demo site's install page | The demo site's install page |
| Companion doc | README, portfolio | [install.md](install.md) |

## Rules for both videos

- **Photos.** Only the web demo's own photos appear, which have GPS and body serial stripped ([SPEC §5.5](SPEC.md#55-web-demo), threat model T17). Never film the desktop app on the author's own folders, never show a real card's contents, and never show a file whose EXIF still holds GPS. In the install video, the first launch stops at the start screen with no folder picked.
- **Nothing personal on screen.** A Windows account named `demo`, no personal bookmarks, browser profile, notifications, taskbar pins, email or other windows. Turn on Do not disturb. Check every frame for user names in paths (`C:\Users\<name>`).
- **Honesty.** The demo video says on screen that the apply in the web demo is simulated. No claim appears in a video that the docs don't make: the safety lines below are taken from the README's promises.
- **Sound.** No copyrighted music. Either no music, or one CC0 or royalty-free track whose licence and source are recorded in the video's description and in this file. Voice-over, if any, is Margaux's own voice or a text-to-speech voice she has approved.
- **Captions.** English and French, as separate WebVTT files (`culla-demo.en.vtt`, `culla-demo.fr.vtt`, the same for the install video), plus burned-in English captions for platforms that autoplay without sound. Each caption stays at most 2 lines of about 42 characters, at most 15 characters per second, on screen for at least 1.5 s; a longer line from the tables below is split into consecutive captions.
- **Accessibility.** No flashing: nothing flashes more than 3 times in any second (WCAG 2.3.1), and cuts are plain cuts or short fades. On-screen text is at least 48 px high at 1080p (about 4.5 % of the frame height), with a contrast of at least 4.5:1 against what is behind it; small UI is shown zoomed in rather than at full-screen size. The cursor moves slowly and pauses before each click. A text transcript (the "voice-over" column below) is published with each video. Key presses are shown as on-screen key labels, so the video makes sense without sound.
- **Format.** 1920×1080, 30 fps, H.264 MP4 (and a WebM copy for the web), stereo AAC at -16 LUFS if there is sound. A poster frame for each video, and a short GIF cut of the demo (shots 4 and 5, under 8 s, under 5 MB) for the README (PRD S7).
- **Look.** Title cards use the Portfolio Design System ([SPEC §5.6](SPEC.md#56-visual-design)): Night (`paper` #0c0b0a, `ink` #f1e9d8) for the demo, Bricolage Grotesque, no gradients or shadows, orange only for the one thing that matters on screen.

## (a) Demo video

About 80 s. Story: the problem, where every photo will go, swiping, changing your mind, the review, an apply that deletes nothing, why it is safe.

### Shot list

| # | Time | Picture | On-screen text (EN) | Voice-over (EN), proposal | Caption (FR), proposal |
|---|---|---|---|---|---|
| 1 | 0:00–0:06 | Title card on Night: a stack of photo cards, the Culla wordmark | After a shoot: hundreds of photos to sort | After a shoot, you come home with hundreds of photos, each a JPEG and a RAW. | Après une séance, des centaines de photos à trier, chacune en JPEG et en RAW. |
| 2 | 0:06–0:12 | Title card: two file icons, `DSC_0412.JPG` and `DSC_0412.NEF`, one sliding away from the other | Sorting by hand, the RAW gets left behind | Sorting them by hand is slow, and the RAW is easy to leave behind. | Trier à la main est lent, et le RAW reste facilement derrière. |
| 3 | 0:12–0:24 | Demo start screen. Pick the demo folder, choose "Move", choose a destination. Slow zoom on the route view: origin, then the Kept branch with one example path (JPEG, NEF, edit file), the Rejected branch (`<source>\_rejected\…`, "new folder", "nothing is deleted"), and "undecided photos stay where they are" | Before you start, see where every photo will go | Before you start, Culla shows where every photo will go: keepers to your folder, rejects set aside, the rest left alone. | Avant de commencer, Culla montre où ira chaque photo : les gardées dans votre dossier, les rejetées mises de côté, le reste ne bouge pas. |
| 4 | 0:24–0:34 | Swipe screen on Night. Drag one photo right (keep feedback in `keep` green), the next one left (dimmed, outline badge) | Swipe right to keep, left to reject | Then look at each photo once. Swipe right to keep it, left to reject it. | Regardez chaque photo une fois. Glissez à droite pour garder, à gauche pour rejeter. |
| 5 | 0:34–0:44 | Press <kbd>→</kbd>, <kbd>←</kbd>, then click the Keep button; key labels appear on screen. Zoom on the route under the buttons showing where this photo would go | Or use the arrow keys and buttons | Or use the arrow keys, or the Keep and Reject buttons. Under each one, Culla shows where this photo would go. | Ou utilisez les flèches, ou les boutons Garder et Rejeter. Sous chacun, Culla montre où irait la photo. |
| 6 | 0:44–0:52 | Press <kbd>Ctrl</kbd>+<kbd>Z</kbd> twice: the view jumps back to the photos just decided, now undecided again | Changed your mind? Ctrl+Z | Changed your mind? Ctrl+Z walks back through your decisions. Nothing has moved yet. | Vous changez d'avis ? Ctrl+Z revient en arrière. Rien n'a encore bougé. |
| 7 | 0:52–1:02 | Review screen: the route view at the top, counts, kept and rejected grids, then one click to the list of operations | Review the whole plan first | When you're done, review the whole plan: what is kept, what is rejected, and every file it will move. | À la fin, relisez tout le plan : ce qui est gardé, ce qui est rejeté, et chaque fichier déplacé. |
| 8 | 1:02–1:12 | Click Apply, the confirm dialog stating the counts, then the apply progress and the report. Lower third: "Web demo: the apply is simulated" | Nothing is deleted | Only then does anything happen. Keepers go to your folder, rejects go to a _rejected folder. Nothing is deleted. | Alors seulement, Culla agit. Les gardées vont dans votre dossier, les rejetées dans _rejected. Rien n'est supprimé. |
| 9 | 1:12–1:20 | End card on Night: Culla wordmark, "Free and open source, for Windows 11", `github.com/margaux-faurie/culla` (and `culla.margauxfaurie.com` once the demo is public) | Nothing overwritten, nothing deleted | Nothing is overwritten, nothing is deleted; every apply is journaled and can resume after a crash. Culla, for Windows 11. | Rien n'est écrasé, rien n'est supprimé ; chaque exécution du plan est journalisée et peut reprendre après un plantage. |

On-screen text is in English in the master. A French master with the on-screen text translated is a later option (SPEC §10).

### How to capture

1. Build the demo (`pnpm build:demo`) at the commit that will be published, and note the commit in the video's description.
2. A Playwright script drives the demo, for example `e2e/video/demo-video.spec.ts` with its own config (`playwright.video.config.ts`, kept out of the CI run): Chromium, viewport 1920×1080, `deviceScaleFactor: 1`, `recordVideo: { dir, size: { width: 1920, height: 1080 } }`, and `trace: "on"` so each take can be inspected. Each shot is one `test.step`, with `page.waitForTimeout` pauses matching the shot list so the edit needs few cuts. Drags use `page.mouse` with many small steps so the swipe physics look natural; key presses use `page.keyboard`. The script only uses the demo's own scenes and photos.
3. Run it with `prefers-reduced-motion` off (the swipe exit is the point of the video), and once with it on to check nothing in the edit depends on motion alone.
4. Edit in a free editor (Shotcut or Kdenlive) or with `ffmpeg`: title cards, zooms on the route view, key labels, the lower third, captions burned in for the social cut. Export the masters, the WebVTT files and the GIF cut.
5. Keep the script, the caption files and the edit project in the repository (`docs/assets/video/`, proposal) so the video can be filmed again when the UI changes; the video files themselves are not committed (size).

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
