# MixLink

Low-latency remote game streaming between two PCs.

- **Host**: the PC that runs the game. Captures screen + audio, encodes, streams.
- **Client**: the PC the player sits at. Decodes, displays, and sends keyboard/mouse/gamepad input back.

Phase 1: LAN with automatic device discovery. Phase 2: over the internet (NAT traversal).
Goal: latency low enough for competitive FPS games.

Start here:
- Architecture: [docs/architecture/overview.md](docs/architecture/overview.md)
- Current status: [docs/STATUS.md](docs/STATUS.md)
- Rules for AI-assisted coding: [CLAUDE.md](CLAUDE.md)

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option.
