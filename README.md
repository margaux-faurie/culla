# Culla

Sort a folder of photos one swipe at a time. Keep or reject each shot, review everything, then apply. Built for Nikon JPEG + RAW (NEF) folders on Windows.

Not ready yet. The design docs are drafted, a few decisions are still open, and the code starts with milestone M0.

- What and why: [docs/PRD.md](docs/PRD.md)
- How it works: [docs/architecture.md](docs/architecture.md) and the [file engine](docs/design/file-engine.md)
- Decisions: [docs/adr/](docs/adr/README.md)
- Plan: [docs/roadmap.md](docs/roadmap.md)
- Everything else: [docs/SPEC.md](docs/SPEC.md) (decisions and document map)

## Promises

- Nothing moves until you confirm.
- No file is ever overwritten.
- Rejected photos go to a `_rejected` folder. Culla never deletes a photo unless a verified copy exists at the destination (the original of a move to another drive).

## License

MIT. See [LICENSE](LICENSE).
