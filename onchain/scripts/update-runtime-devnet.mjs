import fs from "fs";
import path from "path";
import crypto from "crypto";
import os from "os";

import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction, sendAndConfirmTransaction } from "@solana/web3.js";

const repoRoot = path.resolve(process.cwd(), "..");
const envPath = path.join(repoRoot, ".env");
const walletPath = path.join(os.homedir(), ".config", "solana", "id.json");
const CONFIG_SEED = Buffer.from("config");
const CURRENT_BLOCK_SEED = Buffer.from("current_block");

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

function encodeI64(value) {
  const buffer = Buffer.alloc(8);
  buffer.writeBigInt64LE(BigInt(value));
  return buffer;
}

async function main() {
  const env = readEnv(envPath);
  const programId = new PublicKey(env.BLOCKMINE_PROGRAM_ID);
  const connection = new Connection(
    env.NEXT_PUBLIC_SOLANA_RPC_URL || "https://api.devnet.solana.com",
    "confirmed",
  );
  const admin = loadKeypair(walletPath);
  const blockTtlSec = BigInt(env.BLOC_BLOCK_TTL || "60");

  const [configPda] = PublicKey.findProgramAddressSync([CONFIG_SEED], programId);
  const [currentBlockPda] = PublicKey.findProgramAddressSync([CURRENT_BLOCK_SEED], programId);

  const data = Buffer.concat([
    instructionDiscriminator("update_runtime_params"),
    encodeI64(blockTtlSec),
  ]);

  const instruction = new TransactionInstruction({
    programId,
    keys: [
      { pubkey: admin.publicKey, isSigner: true, isWritable: true },
      { pubkey: configPda, isSigner: false, isWritable: true },
      { pubkey: currentBlockPda, isSigner: false, isWritable: true },
    ],
    data,
  });

  const tx = new Transaction().add(instruction);
  const signature = await sendAndConfirmTransaction(connection, tx, [admin], {
    commitment: "confirmed",
  });

  env.BLOC_BLOCK_TTL = blockTtlSec.toString();
  env.BLOC_SUBMIT_FEE_LAMPORTS = "10000000";
  writeEnv(envPath, env);

  console.log(`signature=${signature}`);
  console.log(`config_pda=${configPda.toBase58()}`);
  console.log(`current_block_pda=${currentBlockPda.toBase58()}`);
  console.log(`block_ttl_sec=${blockTtlSec}`);
  console.log("submit_fee_lamports=10000000");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
