import fs from "fs";
import path from "path";
import crypto from "crypto";
import os from "os";

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";

const repoRoot = path.resolve(process.cwd(), "..");
const envPath = path.join(repoRoot, ".env");
const walletPath = path.join(os.homedir(), ".config", "solana", "id.json");
const CONFIG_SEED = Buffer.from("config");

function readEnv(filePath) {
  const values = {};
  if (!fs.existsSync(filePath)) {
    return values;
  }

  for (const line of fs.readFileSync(filePath, "utf8").split(/\r?\n/)) {
    if (!line || line.trim().startsWith("#")) continue;
    const [key, ...rest] = line.split("=");
    values[key.trim()] = rest.join("=").trim();
  }

  return values;
}

function writeEnv(filePath, values) {
  const lines = Object.entries(values).map(([key, value]) => `${key}=${value}`);
  fs.writeFileSync(filePath, `${lines.join("\n")}\n`, "utf8");
}

function loadKeypair(filePath) {
  const secret = JSON.parse(fs.readFileSync(filePath, "utf8"));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

function instructionDiscriminator(name) {
  return crypto.createHash("sha256").update(`global:${name}`).digest().subarray(0, 8);
}

async function ensureAta(connection, payer, mint, owner) {
  const ata = getAssociatedTokenAddressSync(mint, owner, true);
  const info = await connection.getAccountInfo(ata);
  if (info) {
    return ata;
  }

  const tx = new Transaction().add(
    createAssociatedTokenAccountIdempotentInstruction(
      payer.publicKey,
      ata,
      owner,
      mint,
      TOKEN_PROGRAM_ID,
    ),
  );
  await sendAndConfirmTransaction(connection, tx, [payer], {
    commitment: "confirmed",
  });
  return ata;
}

async function main() {
  const env = readEnv(envPath);
  const programId = new PublicKey(env.BLOCKMINE_PROGRAM_ID);
  const connection = new Connection(
    env.NEXT_PUBLIC_SOLANA_RPC_URL || "https://api.devnet.solana.com",
    "confirmed",
  );
  const admin = loadKeypair(walletPath);
  const treasuryAuthority = new PublicKey(env.BLOC_TREASURY_AUTHORITY);
  const mint = new PublicKey(env.BLOC_MINT_ADDRESS);

  const treasuryVault = await ensureAta(connection, admin, mint, treasuryAuthority);
  const [configPda] = PublicKey.findProgramAddressSync([CONFIG_SEED], programId);

  const data = instructionDiscriminator("update_treasury_accounts");
  const instruction = new TransactionInstruction({
    programId,
    keys: [
      { pubkey: admin.publicKey, isSigner: true, isWritable: true },
      { pubkey: configPda, isSigner: false, isWritable: true },
      { pubkey: treasuryAuthority, isSigner: false, isWritable: false },
      { pubkey: treasuryVault, isSigner: false, isWritable: false },
    ],
    data,
  });

  const tx = new Transaction().add(instruction);
  const signature = await sendAndConfirmTransaction(connection, tx, [admin], {
    commitment: "confirmed",
  });

  env.BLOC_TREASURY_VAULT = treasuryVault.toBase58();
  writeEnv(envPath, env);

  console.log(`signature=${signature}`);
  console.log(`config_pda=${configPda.toBase58()}`);
  console.log(`treasury_authority=${treasuryAuthority.toBase58()}`);
  console.log(`treasury_vault=${treasuryVault.toBase58()}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
