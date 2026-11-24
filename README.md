# Culla

Sort a folder of photos one swipe at a time. Keep or reject each shot, review everything, then apply. Built for Nikon JPEG + RAW (NEF) folders on Windows.

Not ready yet. The project is in its design phase.

- Specification: [docs/SPEC.md](docs/SPEC.md)
- Decisions: [docs/adr/](docs/adr/)
- Product brief, architecture, file-engine design and roadmap: in progress

## Promises

- Nothing moves until you confirm.
- No file is ever overwritten.
- Rejected photos go to a `_rejected` folder. Culla never deletes a photo unless a verified copy exists at the destination (the original of a move to another drive).

## License

MIT. See [LICENSE](LICENSE).
