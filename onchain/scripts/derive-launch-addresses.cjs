const { PublicKey } = require("@solana/web3.js");
const { getAssociatedTokenAddressSync } = require("@solana/spl-token");

function must(name, value) {
  if (!value) {
    throw new Error(`Missing ${name}`);
  }
  return value;
}

const programId = new PublicKey(must("PROGRAM_ID", process.env.PROGRAM_ID));
const mint = new PublicKey(must("MINT", process.env.MINT));
const treasuryAuthority = new PublicKey(
  must("TREASURY_AUTHORITY", process.env.TREASURY_AUTHORITY),
);
const lpOwner = new PublicKey(must("LP_OWNER", process.env.LP_OWNER));

const [vaultAuthority] = PublicKey.findProgramAddressSync(
  [Buffer.from("vault_authority")],
  programId,
);

const rewardVault = getAssociatedTokenAddressSync(mint, vaultAuthority, true);
const treasuryVault = getAssociatedTokenAddressSync(mint, treasuryAuthority, true);
const lpAta = getAssociatedTokenAddressSync(mint, lpOwner, true);

console.log(`vault_authority=${vaultAuthority.toBase58()}`);
console.log(`reward_vault=${rewardVault.toBase58()}`);
console.log(`treasury_authority=${treasuryAuthority.toBase58()}`);
console.log(`treasury_vault=${treasuryVault.toBase58()}`);
console.log(`lp_owner=${lpOwner.toBase58()}`);
console.log(`lp_ata=${lpAta.toBase58()}`);
