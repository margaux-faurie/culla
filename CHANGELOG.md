# Changelog

## 1.0.0 (2026-11-28)


### Features

* **bench:** culla-bench for the M1 benchmark campaign ([#30](https://github.com/margaux-faurie/culla/issues/30)) ([370dd7b](https://github.com/margaux-faurie/culla/commit/370dd7b8022ed98eb1a1bb05a673481658e1548f))
* **cli:** culla plan, a read-only dry run over plan() and preflight ([#40](https://github.com/margaux-faurie/culla/issues/40)) ([3e13ac9](https://github.com/margaux-faurie/culla/commit/3e13ac9d67e30efd1eb67ce6b08bf08cccfa4440))
* **cli:** culla scan with text and JSON reports ([#26](https://github.com/margaux-faurie/culla/issues/26)) ([f9930e6](https://github.com/margaux-faurie/culla/commit/f9930e64b1e198de8f6775734d8000e62b895ada))
* **core:** bounded TIFF/IFD walker and NEF preview finder ([#18](https://github.com/margaux-faurie/culla/issues/18)) ([085f096](https://github.com/margaux-faurie/culla/commit/085f096f57c886107e7894085d5a77e0ed1d1c6e))
* **core:** decision event log with golden fixtures ([#32](https://github.com/margaux-faurie/culla/issues/32)) ([5e1e6f4](https://github.com/margaux-faurie/culla/commit/5e1e6f44d1a0c100456cea7457fbc223cf1cab4d))
* **core:** EXIF capture time, orientation and lens; capture order ([#23](https://github.com/margaux-faurie/culla/issues/23)) ([62c77b5](https://github.com/margaux-faurie/culla/commit/62c77b508d60d21a43dc24e6e55d8ab96e956197))
* **core:** group scanned files; streamed read-only folder scan ([#20](https://github.com/margaux-faurie/culla/issues/20)) ([af64bd6](https://github.com/margaux-faurie/culla/commit/af64bd673dc9643c39a582fe04fdc0ece00f1bd0))
* **core:** journal state machine, recovery table and one executor for apply and resume ([#36](https://github.com/margaux-faurie/culla/issues/36)) ([0c62d28](https://github.com/margaux-faurie/culla/commit/0c62d28960a6544e7c97682a7e736351dfe671fe))
* **core:** plain-text apply log renderer ([#41](https://github.com/margaux-faurie/culla/issues/41)) ([137158e](https://github.com/margaux-faurie/culla/commit/137158edee7eb2fbc6ebdbdeaa71e7e37f4dbf80))
* **core:** plan() for keep, reject, RAW follows and sidecars ([#33](https://github.com/margaux-faurie/culla/issues/33)) ([035447e](https://github.com/margaux-faurie/culla/commit/035447e0edfc9f67e4e5761420a2669547b2ae70))
* **core:** preflight rules P1 to P17 over a plan and a snapshot of facts ([#35](https://github.com/margaux-faurie/culla/issues/35)) ([b67473d](https://github.com/margaux-faurie/culla/commit/b67473df0af2255259bdef2178d538e23825e292))
* **core:** volume identity and PC time zone for the file-time fallback ([#25](https://github.com/margaux-faurie/culla/issues/25)) ([5903a80](https://github.com/margaux-faurie/culla/commit/5903a8063f87fe33c5dc8d2dc2ff43ed9f4f40a3))
* **demo:** publish the 30 s demo video on the install page ([#57](https://github.com/margaux-faurie/culla/issues/57)) ([f7140bc](https://github.com/margaux-faurie/culla/commit/f7140bc6a2cee51204e6096cab26315d04bddbbc))
* **preview:** prefetch policy, bounded decode pool, in-memory cache and culla preview ([#29](https://github.com/margaux-faurie/culla/issues/29)) ([b0aa978](https://github.com/margaux-faurie/culla/commit/b0aa978b25ac6b6d3b3e6efc2976d3f7e1fe497e))
* **preview:** scaled decode, resize, orient and encode for one preview ([#28](https://github.com/margaux-faurie/culla/issues/28)) ([37e37ba](https://github.com/margaux-faurie/culla/commit/37e37ba36f8b7eacd400a79c3bc8b036c265ca19))
* **ui:** apply screen, report and simulated run on the demo backend ([#44](https://github.com/margaux-faurie/culla/issues/44)) ([30f977e](https://github.com/margaux-faurie/culla/commit/30f977e4d55f553f8213b602a8879afff3aa434c))
* **ui:** EXIF overlay on the swipe screen (US-40) ([#55](https://github.com/margaux-faurie/culla/issues/55)) ([7983cc4](https://github.com/margaux-faurie/culla/commit/7983cc4cbc20c4b114b0972acddc5dbb2e3ed245))
* **ui:** lean review and short route on apply; a per-PR video freshness gate ([#56](https://github.com/margaux-faurie/culla/issues/56)) ([a0557c0](https://github.com/margaux-faurie/culla/commit/a0557c063eb80188ec81264b80e64ddcdfd06694))
* **ui:** resume a folder's session, or start it over (US-37) ([#52](https://github.com/margaux-faurie/culla/issues/52)) ([ec7939d](https://github.com/margaux-faurie/culla/commit/ec7939db73eec35cc4adf1cb3e05162d426c15f8))
* **ui:** review screen and confirm dialog on the demo backend ([#43](https://github.com/margaux-faurie/culla/issues/43)) ([d49e39a](https://github.com/margaux-faurie/culla/commit/d49e39ace415154a758e46eead07a0ee3ecfddd7))
* **ui:** start and swipe screens on the demo backend ([#34](https://github.com/margaux-faurie/culla/issues/34)) ([f213bf8](https://github.com/margaux-faurie/culla/commit/f213bf86ec953a28ffe44172519d018a507eaa3c))
* **ui:** start sorting from the folder, choose move or copy and destination on review ([#47](https://github.com/margaux-faurie/culla/issues/47)) ([71be6a9](https://github.com/margaux-faurie/culla/commit/71be6a9c62fc0d5abae3ecfb7336319b77bbb44b))
* **ui:** zoom to 100 % on the swipe screen (US-39) ([#54](https://github.com/margaux-faurie/culla/issues/54)) ([02d319d](https://github.com/margaux-faurie/culla/commit/02d319d7661ccc740afb90cba132133f266328a9))


### Bug Fixes

* **core:** a stop settles groups left mid-way and is kept across a crash ([#45](https://github.com/margaux-faurie/culla/issues/45)) ([9a09a82](https://github.com/margaux-faurie/culla/commit/9a09a82a5bf2c2d5a87f1aaed331663e01d768ee))
* **demo:** ship placeholder photos as files so img-src 'self' allows them ([#53](https://github.com/margaux-faurie/culla/issues/53)) ([9ba87ed](https://github.com/margaux-faurie/culla/commit/9ba87ed327aa894702fdb8553f02f026f26a0572))
* **ui:** a late close event no longer shuts the confirm dialog opened again ([#51](https://github.com/margaux-faurie/culla/issues/51)) ([a02cf83](https://github.com/margaux-faurie/culla/commit/a02cf83d9e95102c8c5ef5bf94ec8a47fcc7a51e))
* **ui:** self-host Bricolage Grotesque for the app and the demo ([#50](https://github.com/margaux-faurie/culla/issues/50)) ([81d5b60](https://github.com/margaux-faurie/culla/commit/81d5b60fb5491ce4283c4a0a1c1f48d8633f5912))
