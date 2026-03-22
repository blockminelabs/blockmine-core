# macOS Miner Packaging

This folder packages the same shared Blockmine miner source into a single macOS app:

- one app
- CPU and GPU selectable inside the UI
- one DMG

Important:
- the source of truth is still `miner-client/`
- GPU on Apple Silicon is still experimental because the current backend is OpenCL-based
- CPU mode is the reliable path today

Run on a real Mac:

```bash
chmod +x packaging/macos/*.command packaging/macos/scripts/*.sh
./packaging/macos/build-macos.command
```

Optional GPU/OpenCL device check:

```bash
./packaging/macos/test-gpu-devices.command
```

Artifacts end up in:
- `dist/Blockmine Miner.app`
- `dist/Blockmine Miner.dmg`
