# Windows Miner Packaging

Build the Windows GUI miner from the shared codebase with:

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\build-miner-exe.ps1
```

Artifacts end up in:
- `dist/miner.exe`
- `dist/start-blockmine-studio.bat`
- `dist/README-blockmine-studio.txt`

The actual source of truth remains:
- `miner-client/`
- `onchain/`
