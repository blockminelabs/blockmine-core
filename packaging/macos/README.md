# macOS Packaging

Build on a Mac from the repository root:

```bash
chmod +x packaging/macos/*.command
chmod +x packaging/macos/scripts/*.sh
./packaging/macos/build-macos.command
```

Artifacts:

- `dist/Blockmine Miner.app`
- `dist/Blockmine Miner.dmg`

GPU support on macOS depends on an OpenCL-capable host runtime. CPU mode remains the baseline path.
