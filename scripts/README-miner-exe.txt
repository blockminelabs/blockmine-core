Blockmine Miner Windows Build

Public binary:
- dist/Blockmine Miner.exe

Mainnet defaults:
- Program ID: FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv
- Mint: 9AJa38FiS8kD2n2Ztubrk6bCSYt55Lz2fBye3Comu1mg

Rebuild:
- powershell -ExecutionPolicy Bypass -File .\scripts\build-miner-exe.ps1

The packaged desktop miner includes:
- wallet manager
- wallet import by mnemonic or private key
- QR-assisted manual funding
- CPU, GPU, and hybrid mining
- live protocol telemetry

dist/ is a build artifact and should not be committed.
