# Video changes pending

Screens the demo video films (`screens.json`) that changed since its last render, to film again before the next release ([videos.md](../../videos.md#keeping-the-video-current)). A pull request that changes one of those screens adds a line here, unless it renders the video again. One line per change, in this form:

`- <date> #<PR or branch>: <screen> changed — re-render before release`

The render (`node docs/assets/video/hash.mjs --write`) empties the list; the release waits until it is empty.

- 2026-11-28 #50: all screens (self-hosted font) changed — re-render before release
- 2026-11-28 #51: review (confirm dialog) changed — re-render before release
- 2026-11-28 #53: demo photos changed — re-render before release
- 2026-11-28 #54: swipe (zoom to 100 %) changed — re-render before release
- 2026-11-28 #55: swipe (EXIF overlay) changed — re-render before release
- 2026-11-28 #56: review (lean layout) changed — re-render before release
