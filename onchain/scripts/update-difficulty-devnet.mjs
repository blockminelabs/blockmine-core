import fs from "fs";
import path from "path";
import crypto from "crypto";
import os from "os";

import { Connection, PublicKey, Transaction, TransactionInstruction, sendAndConfirmTransaction, Keypair } from "@solana/web3.js";

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

function encodeU64(value) {
  const buffer = Buffer.alloc(8);
  buffer.writeBigUInt64LE(BigInt(value));
  return buffer;
}

function encodeU8(value) {
  const buffer = Buffer.alloc(1);
  buffer.writeUInt8(Number(value));
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

  const targetBlockTimeSec = BigInt(env.BLOC_TARGET_BLOCK_TIME || "10");
  const adjustmentInterval = BigInt(env.BLOC_ADJUSTMENT_INTERVAL || "1");
  const difficultyBits = Number(env.BLOC_INITIAL_DIFFICULTY_BITS || "24");
  const minDifficultyBits = Number(env.BLOC_MIN_DIFFICULTY_BITS || "12");
  const maxDifficultyBits = Number(env.BLOC_MAX_DIFFICULTY_BITS || "40");

  const [configPda] = PublicKey.findProgramAddressSync([CONFIG_SEED], programId);
  const [currentBlockPda] = PublicKey.findProgramAddressSync([CURRENT_BLOCK_SEED], programId);

  const data = Buffer.concat([
    instructionDiscriminator("update_difficulty_params"),
    encodeU64(targetBlockTimeSec),
    encodeU64(adjustmentInterval),
    encodeU8(difficultyBits),
    encodeU8(minDifficultyBits),
    encodeU8(maxDifficultyBits),
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

  env.BLOC_TARGET_BLOCK_TIME = targetBlockTimeSec.toString();
  env.BLOC_ADJUSTMENT_INTERVAL = adjustmentInterval.toString();
  env.BLOC_INITIAL_DIFFICULTY_BITS = difficultyBits.toString();
  env.BLOC_MIN_DIFFICULTY_BITS = minDifficultyBits.toString();
  env.BLOC_MAX_DIFFICULTY_BITS = maxDifficultyBits.toString();
  writeEnv(envPath, env);

  console.log(`signature=${signature}`);
  console.log(`config_pda=${configPda.toBase58()}`);
  console.log(`current_block_pda=${currentBlockPda.toBase58()}`);
  console.log(`target_block_time_sec=${targetBlockTimeSec}`);
  console.log(`adjustment_interval=${adjustmentInterval}`);
  console.log(`difficulty_bits=${difficultyBits}`);
  console.log(`min_difficulty_bits=${minDifficultyBits}`);
  console.log(`max_difficulty_bits=${maxDifficultyBits}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
