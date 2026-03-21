import fs from "fs";
import path from "path";

import { Connection, PublicKey } from "@solana/web3.js";
import { getAccount, getMint } from "@solana/spl-token";

const repoRoot = path.resolve(process.cwd(), "..");
const envPath = path.join(repoRoot, ".env");
const CONFIG_SEED = Buffer.from("config");
const CURRENT_BLOCK_SEED = Buffer.from("current_block");
const VAULT_AUTHORITY_SEED = Buffer.from("vault_authority");

function readEnv(filePath) {
  const values = {};
  if (!fs.existsSync(filePath)) {
    return values;
  }

  for (const line of fs.readFileSync(filePath, "utf8").split(/\r?\n/)) {
    if (!line || line.trim().startsWith("#")) {
      continue;
    }
    const [key, ...rest] = line.split("=");
    values[key.trim()] = rest.join("=").trim();
  }

  return values;
}

async function main() {
  const env = readEnv(envPath);
  const connection = new Connection(
    env.NEXT_PUBLIC_SOLANA_RPC_URL || "https://api.devnet.solana.com",
    "confirmed",
  );
  const programId = new PublicKey(env.BLOCKMINE_PROGRAM_ID);
  const mint = new PublicKey(env.BLOC_MINT_ADDRESS);
  const rewardVault = new PublicKey(env.BLOC_REWARD_VAULT);
  const [configPda] = PublicKey.findProgramAddressSync([CONFIG_SEED], programId);
  const [currentBlockPda] = PublicKey.findProgramAddressSync([CURRENT_BLOCK_SEED], programId);
  const [vaultAuthorityPda] = PublicKey.findProgramAddressSync([VAULT_AUTHORITY_SEED], programId);

  const [mintInfo, rewardVaultInfo, configInfo, currentBlockInfo] = await Promise.all([
    getMint(connection, mint),
    getAccount(connection, rewardVault),
    connection.getAccountInfo(configPda),
    connection.getAccountInfo(currentBlockPda),
  ]);

  console.log(
    JSON.stringify(
      {
        programId: programId.toBase58(),
        configPda: configPda.toBase58(),
        currentBlockPda: currentBlockPda.toBase58(),
        vaultAuthorityPda: vaultAuthorityPda.toBase58(),
        mint: mint.toBase58(),
        rewardVault: rewardVault.toBase58(),
        supply: mintInfo.supply.toString(),
        decimals: mintInfo.decimals,
        mintAuthority: mintInfo.mintAuthority ? mintInfo.mintAuthority.toBase58() : null,
        freezeAuthority: mintInfo.freezeAuthority ? mintInfo.freezeAuthority.toBase58() : null,
        rewardVaultAmount: rewardVaultInfo.amount.toString(),
        rewardVaultOwner: rewardVaultInfo.owner.toBase58(),
        configExists: Boolean(configInfo),
        currentBlockExists: Boolean(currentBlockInfo),
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
