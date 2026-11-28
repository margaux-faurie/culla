# Video changes pending

Screens the demo video films (`screens.json`) that changed since its last render, to film again ([videos.md](../../videos.md#keeping-the-video-current)). Every pull request that changes one of those screens acknowledges it (Margaux, 28 Nov 2026): it renders the video again, or it adds its line here with

`node docs/assets/video/hash.mjs --pending "<date> #<PR>: <screen> changed — re-render before release"`

which also records the hash it acknowledged (`pendingFrom` in `screens.json`); `tests/video-freshness.test.ts` fails otherwise. A render (`node docs/assets/video/hash.mjs --write`) empties the list. Proposed, awaiting Margaux: the release waits until the list is empty.
