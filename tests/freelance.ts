import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Freelance } from "../target/types/freelance";

// Custom provider setup with retry logic
const setupProvider = () => {
  const provider = anchor.AnchorProvider.env();
  const connection = new anchor.web3.Connection(provider.connection.rpcEndpoint, {
    commitment: "confirmed",
    confirmTransactionInitialTimeout: 60000,
  });
  return new anchor.AnchorProvider(connection, provider.wallet, {
    commitment: "confirmed",
    preflightCommitment: "confirmed",
  });
};

// Set custom provider
anchor.setProvider(setupProvider());

const program = anchor.workspace.Freelance as Program<Freelance>;

describe("freelance", () => {
  // Add connection check before tests
  before(async () => {
    const provider = anchor.AnchorProvider.env();
    try {
      await provider.connection.getLatestBlockhash();
    } catch (e) {
      console.error("Failed to connect to validator. Please ensure it's running on the correct port.");
      throw e;
    }
  });

  const provider = anchor.AnchorProvider.env();
  const wallet = provider.wallet;

  // Generate a new keypair for the protocol account
  const protocolAccount = anchor.web3.Keypair.generate();
  const userInfo = anchor.web3.Keypair.generate();

  it("Is initialized!", async () => {
    console.log("Initializing...");
    const tx = await program.methods.initialize()
      .accounts({
        protocol: protocolAccount.publicKey,
        userInfo: userInfo.publicKey,
        owner: wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([protocolAccount, userInfo])
      .rpc();
    console.log("Your transaction signature", tx);
  });

  it("Gets pool length", async () => {
    console.log("Fetching pool accounts...");
    const poolAccounts = await program.account.pool.all();  // Fetch all pool accounts
    console.log("Pool accounts:", poolAccounts);
    console.log("Pool length:", poolAccounts.length);
  });

  it("Fetches deposits pool length", async () => {
    const pid = new anchor.BN(0);
    const depositsPoolLength = await program.methods.depositsPoolLength(pid)
      .accounts({
        protocol: protocolAccount.publicKey,
        userInfo: userInfo.publicKey,
        pool: protocolAccount.publicKey, // placeholder: replace with actual pool account if available
      })
      .rpc();
    console.log("Deposits pool length:", depositsPoolLength);
  });

  it("Fetches available sum for withdraw", async () => {
    const pid = new anchor.BN(0);
    const availableSum = await program.methods.getAvailableSumForWithdraw(pid)
      .accounts({
        protocol: protocolAccount.publicKey,
        userInfo: userInfo.publicKey,
        pool: protocolAccount.publicKey, // placeholder
      })
      .rpc();
    console.log("Available sum for withdraw:", availableSum);
  });

  it("Fetches claimable amount", async () => {
    const pid = new anchor.BN(0);
    const claimable = await program.methods.getClaimable(pid)
      .accounts({
        protocol: protocolAccount.publicKey,
        userInfo: userInfo.publicKey,
        pool: protocolAccount.publicKey, // placeholder
      })
      .rpc();
    console.log("Claimable amount:", claimable);
  });

  it("Fetches pool rate and APY", async () => {
    const pid = new anchor.BN(0);
    const timestamp = new anchor.BN(Math.floor(Date.now() / 1000));
    const poolRateAndApy = await program.methods.getPoolRateAndApy(pid, timestamp)
      .accounts({
        protocol: protocolAccount.publicKey,
        pool: protocolAccount.publicKey, // placeholder
      })
      .rpc();
    console.log("Pool rate and APY:", poolRateAndApy);
  });

  it("Gets pool accounts", async () => {
    console.log("Fetching pool accounts...");
    const poolAccounts = await program.account.pool.all();  // Fetch all pool accounts
    console.log("Pool accounts:", poolAccounts);
  });
});
