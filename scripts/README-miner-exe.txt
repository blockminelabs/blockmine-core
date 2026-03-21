BlockMine Miner EXE

Files:
- blockmine-miner.exe
- start-miner-devnet.bat

Default Devnet launch:
- double click start-miner-devnet.bat
- il launcher chiede solo l'address pubblico del wallet e poi verifica che corrisponda al keypair locale
- l'address incollato deve combaciare con il file keypair configurato

If your wallet is not in:
- %USERPROFILE%\.config\solana\id.json

you can either:
- set SOLANA_WALLET before launch
- or run the exe manually with --keypair

Examples:
- blockmine-miner.exe protocol-state
- blockmine-miner.exe wallet-stats --keypair "C:\path\to\id.json"
- blockmine-miner.exe mine --backend cpu --batch-size 500000 --keypair "C:\path\to\id.json"
- blockmine-miner.exe mine --backend both --batch-size 250000 --gpu-batch-size 131072 --keypair "C:\path\to\id.json"

Current Devnet program:
- HQCgF9XWsJPH3uEfRdRGW1rARwWqDpV361ZpaXUostfw
