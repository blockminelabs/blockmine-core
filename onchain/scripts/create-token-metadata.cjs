#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const { Keypair, PublicKey } = require("@solana/web3.js");
const { mplTokenMetadata, createMetadataAccountV3 } = require("@metaplex-foundation/mpl-token-metadata");
const { createUmi } = require("@metaplex-foundation/umi-bundle-defaults");
const {
  publicKey,
  signerIdentity,
  createSignerFromKeypair,
  none,
  percentAmount,
  base58,
} = require("@metaplex-foundation/umi");
const { fromWeb3JsKeypair } = require("@metaplex-foundation/umi-web3js-adapters");

const METADATA_PROGRAM_ID = new PublicKey("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

function loadKeypair(filePath) {
  const raw = JSON.parse(fs.readFileSync(filePath, "utf8"));
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

async function main() {
  const rpcUrl = process.env.SOLANA_RPC_URL ?? "https://api.mainnet-beta.solana.com";
  const mint = process.env.BLOC_MINT_ADDRESS;
  const uri = process.env.BLOC_METADATA_URI;
  const name = process.env.BLOC_TOKEN_NAME ?? "Blockmine";
  const symbol = process.env.BLOC_TOKEN_SYMBOL ?? "BLOC";
  const keypairPath =
    process.env.BLOC_MINT_AUTHORITY_KEYPAIR ?? path.resolve(__dirname, "../../../Launch/wallets/mint-authority.json");

  if (!mint) {
    throw new Error("BLOC_MINT_ADDRESS is required");
  }
  if (!uri) {
    throw new Error("BLOC_METADATA_URI is required");
  }

  const payer = loadKeypair(keypairPath);
  const umi = createUmi(rpcUrl).use(mplTokenMetadata());
  const signer = createSignerFromKeypair(umi, fromWeb3JsKeypair(payer));
  umi.use(signerIdentity(signer));

  const mintPublicKey = publicKey(mint);
  const [metadataPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("metadata"), METADATA_PROGRAM_ID.toBuffer(), new PublicKey(mint).toBuffer()],
    METADATA_PROGRAM_ID,
  );
  const metadataPdaPublicKey = publicKey(metadataPda.toBase58());

  const metadataAccount = await umi.rpc.getAccount(metadataPdaPublicKey, { commitment: "confirmed" });
  if (metadataAccount.exists) {
    console.log(`metadata_pda=${metadataPda.toBase58()}`);
    console.log("metadata_exists=true");
    return;
  }

  const builder = createMetadataAccountV3(umi, {
    mint: mintPublicKey,
    metadata: metadataPdaPublicKey,
    mintAuthority: signer,
    payer: signer,
    updateAuthority: signer,
    data: {
      name,
      symbol,
      uri,
      sellerFeeBasisPoints: percentAmount(0),
      creators: none(),
      collection: none(),
      uses: none(),
    },
    isMutable: true,
    collectionDetails: none(),
  });

  const result = await builder.sendAndConfirm(umi, { confirm: { commitment: "confirmed" } });
  const signature =
    typeof result.signature === "string" ? result.signature : base58.serialize(result.signature);
  console.log(`metadata_pda=${metadataPda.toBase58()}`);
  console.log(`signature=${signature}`);
  console.log("metadata_exists=true");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
