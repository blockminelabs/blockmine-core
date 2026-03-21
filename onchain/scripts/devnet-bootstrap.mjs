import fs from "fs";
import path from "path";
import crypto from "crypto";
import os from "os";

import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  AuthorityType,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createMint,
  getAssociatedTokenAddressSync,
  mintTo,
  setAuthority,
} from "@solana/spl-token";

const repoRoot = path.resolve(process.cwd(), "..");
const envPath = path.join(repoRoot, ".env");
const walletPath = path.join(os.homedir(), ".config", "solana", "id.json");

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

function encodeI64(value) {
  const buffer = Buffer.alloc(8);
  buffer.writeBigInt64LE(BigInt(value));
  return buffer;
}

function encodeU16(value) {
  const buffer = Buffer.alloc(2);
  buffer.writeUInt16LE(Number(value));
  return buffer;
}

function encodeU8(value) {
  const buffer = Buffer.alloc(1);
  buffer.writeUInt8(Number(value));
  return buffer;
}

function initializeProtocolData(args) {
  return Buffer.concat([
    instructionDiscriminator("initialize_protocol"),
    encodeU64(args.maxSupply),
    encodeU64(args.initialBlockReward),
    encodeU16(args.treasuryFeeBps),
    encodeU64(args.halvingInterval),
    encodeU64(args.targetBlockTimeSec),
    encodeU64(args.adjustmentInterval),
    encodeU8(args.initialDifficultyBits),
    encodeU8(args.minDifficultyBits),
    encodeU8(args.maxDifficultyBits),
    encodeU64(args.submitFeeLamports),
    encodeI64(args.blockTtlSec),
    encodeU8(args.tokenDecimals),
  ]);
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
  const payer = loadKeypair(walletPath);
  const treasuryAuthority = env.BLOC_TREASURY_AUTHORITY
    ? new PublicKey(env.BLOC_TREASURY_AUTHORITY)
    : payer.publicKey;

  const [configPda] = PublicKey.findProgramAddressSync([CONFIG_SEED], programId);
  const [currentBlockPda] = PublicKey.findProgramAddressSync([CURRENT_BLOCK_SEED], programId);
  const [vaultAuthorityPda] = PublicKey.findProgramAddressSync([VAULT_AUTHORITY_SEED], programId);

  let mintPubkey;
  if (env.BLOC_MINT_ADDRESS) {
    mintPubkey = new PublicKey(env.BLOC_MINT_ADDRESS);
  } else {
    mintPubkey = await createMint(
      connection,
      payer,
      payer.publicKey,
      payer.publicKey,
      Number(env.BLOC_TOKEN_DECIMALS || 9),
    );
    env.BLOC_MINT_ADDRESS = mintPubkey.toBase58();
  }

  const rewardVault = await ensureAta(connection, payer, mintPubkey, vaultAuthorityPda);
  const treasuryVault = await ensureAta(connection, payer, mintPubkey, treasuryAuthority);

  const configInfo = await connection.getAccountInfo(configPda);
  if (!configInfo) {
    const initIx = new TransactionInstruction({
      programId,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: mintPubkey, isSigner: false, isWritable: false },
        { pubkey: configPda, isSigner: false, isWritable: true },
        { pubkey: currentBlockPda, isSigner: false, isWritable: true },
        { pubkey: vaultAuthorityPda, isSigner: false, isWritable: false },
        { pubkey: rewardVault, isSigner: false, isWritable: false },
        { pubkey: treasuryAuthority, isSigner: false, isWritable: false },
        { pubkey: treasuryVault, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: initializeProtocolData({
        maxSupply: env.BLOC_MAX_SUPPLY || "20000000000000000",
        initialBlockReward: env.BLOC_INITIAL_BLOCK_REWARD || "21000000000",
        treasuryFeeBps: env.BLOC_TREASURY_FEE_BPS || "100",
        halvingInterval: env.BLOC_HALVING_INTERVAL || "210000",
        targetBlockTimeSec: env.BLOC_TARGET_BLOCK_TIME || "10",
        adjustmentInterval: env.BLOC_ADJUSTMENT_INTERVAL || "1",
        initialDifficultyBits: env.BLOC_INITIAL_DIFFICULTY_BITS || "24",
        minDifficultyBits: env.BLOC_MIN_DIFFICULTY_BITS || "12",
        maxDifficultyBits: env.BLOC_MAX_DIFFICULTY_BITS || "40",
        submitFeeLamports: env.BLOC_SUBMIT_FEE_LAMPORTS || "10000000",
        blockTtlSec: env.BLOC_BLOCK_TTL || "60",
        tokenDecimals: env.BLOC_TOKEN_DECIMALS || "9",
      }),
    });

    const initTx = new Transaction().add(initIx);
    await sendAndConfirmTransaction(connection, initTx, [payer], {
      commitment: "confirmed",
    });
  }

  const rewardVaultInfo = await connection.getTokenAccountBalance(rewardVault);
  if (rewardVaultInfo.value.amount === "0") {
    await mintTo(
      connection,
      payer,
      mintPubkey,
      rewardVault,
      payer.publicKey,
      BigInt(env.BLOC_MAX_SUPPLY || "20000000000000000"),
    );
  }

  await setAuthority(
    connection,
    payer,
    mintPubkey,
    payer.publicKey,
    AuthorityType.MintTokens,
    null,
  );

  await setAuthority(
    connection,
    payer,
    mintPubkey,
    payer.publicKey,
    AuthorityType.FreezeAccount,
    null,
  );

  env.BLOC_MINT_ADDRESS = mintPubkey.toBase58();
  env.BLOC_REWARD_VAULT = rewardVault.toBase58();
  env.BLOC_TREASURY_AUTHORITY = treasuryAuthority.toBase58();
  env.BLOC_TREASURY_VAULT = treasuryVault.toBase58();
  writeEnv(envPath, env);

  console.log(`program_id=${programId.toBase58()}`);
  console.log(`wallet=${payer.publicKey.toBase58()}`);
  console.log(`mint=${mintPubkey.toBase58()}`);
  console.log(`config_pda=${configPda.toBase58()}`);
  console.log(`current_block_pda=${currentBlockPda.toBase58()}`);
  console.log(`reward_vault=${rewardVault.toBase58()}`);
  console.log(`treasury_authority=${treasuryAuthority.toBase58()}`);
  console.log(`treasury_vault=${treasuryVault.toBase58()}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
