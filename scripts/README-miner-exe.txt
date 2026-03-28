Blockmine Miner Windows Build

Current packaged binary:
- dist/Blockmine Miner.exe

Current public mainnet defaults:
- Program ID: FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv
- Mint: 9AJa38FiS8kD2n2Ztubrk6bCSYt55Lz2fBye3Comu1mg

How to rebuild:
- powershell -ExecutionPolicy Bypass -File .\scripts\build-miner-exe.ps1

What the packaged desktop miner includes:
- branded executable icon
- wallet manager
- create/import wallet flows
- QR-assisted manual funding
- CPU, GPU, and hybrid mining modes

Important:
- dist/ is a build artifact and is not meant to be committed
- launch wallets, private keys, and runbooks do not belong in this repo
- devnet rehearsal tooling exists elsewhere in the repo, but the packaged desktop client defaults to mainnet
