# Blockmine Miner Packaging

This folder keeps platform-specific packaging flows while the actual source code stays shared in:

- `miner-client/`
- `onchain/`

That means:
- Windows and macOS builds use the same Rust codebase
- future miner changes should happen in `miner-client/`
- packaging scripts in `packaging/windows` and `packaging/macos` only wrap build and distribution steps

Current structure:
- `windows/`: Windows build helpers that produce `dist/miner.exe`
- `macos/`: macOS app and DMG packaging helpers

Rule of thumb:
- change product logic/UI once in `miner-client`
- rebuild/package per platform from here
