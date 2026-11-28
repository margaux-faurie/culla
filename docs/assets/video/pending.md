# Video changes pending

Screens the demo video films (`screens.json`) that changed since its last render, to film again ([videos.md](../../videos.md#keeping-the-video-current)). Every pull request that changes one of those screens acknowledges it (Margaux, 28 Nov 2026): it renders the video again, or it adds its line here with

`node docs/assets/video/hash.mjs --pending "<date> #<PR>: <screen> changed — re-render before release"`

which also records the hash it acknowledged (`pendingFrom` in `screens.json`); `tests/video-freshness.test.ts` fails otherwise. A render (`node docs/assets/video/hash.mjs --write`) empties the list. Proposed, awaiting Margaux: the release waits until the list is empty.

- 2026-11-28 #50: all screens (self-hosted font) changed — re-render before release
- 2026-11-28 #51: review (confirm dialog) changed — re-render before release
- 2026-11-28 #52: start and swipe (resume, start over) changed — re-render before release
- 2026-11-28 #53: demo photos changed — re-render before release
- 2026-11-28 #54: swipe (zoom to 100 %) changed — re-render before release
- 2026-11-28 #55: swipe (EXIF overlay) changed — re-render before release
- 2026-11-28 #57: install page styles (transcript, in app.css) changed — re-render before release
- 2026-11-28 #56: review and apply (lean review, short route) changed — re-render before release
