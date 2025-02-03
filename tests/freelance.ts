import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Freelance } from "../target/types/freelance";

// Custom provider setup with retry logic
const setupProvider = () => {
  const provider = anchor.AnchorProvider.env();
  // Use the correct RPC port
  const connection = new anchor.web3.Connection("http://127.0.0.1:8899", {
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

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

describe("freelance", () => {
  // Generate new keypairs for each test run
  const protocolAccount = anchor.web3.Keypair.generate();
  const userInfo = anchor.web3.Keypair.generate();
  const provider = setupProvider(); // Use the custom provider
  const wallet = provider.wallet as anchor.Wallet;

  let isInitialized = false;

  before(async () => {
    try {
      // Clean up any existing accounts
      try {
        await provider.connection.requestAirdrop(wallet.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
        await sleep(1000); // Wait for airdrop to be confirmed
      } catch (e) {
        console.warn("Airdrop failed, continuing...", e);
      }

      const latestBlockhash = await provider.connection.getLatestBlockhash();
      console.log("Connected to validator, blockhash:", latestBlockhash.blockhash);
    } catch (e) {
      console.error("Failed to connect to validator:", e);
      throw e;
    }
  });

  it("Is initialized!", async () => {
    console.log("Starting initialization...");
    console.log("Protocol account pubkey:", protocolAccount.publicKey.toString());
    console.log("User info pubkey:", userInfo.publicKey.toString());
    console.log("Wallet pubkey:", wallet.publicKey.toString());

    try {
      // Initialize the protocol using Anchor's account initialization
      const tx = await program.methods.initialize()
        .accounts({
          protocol: protocolAccount.publicKey,
          userInfo: userInfo.publicKey,
          owner: wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([protocolAccount, userInfo])
        .rpc();

      // Wait for transaction confirmation
      await provider.connection.confirmTransaction(tx);
      console.log("Your transaction signature", tx);

      // Wait a bit for the account to be available
      await sleep(1000);

      // Verify that both protocol and user info accounts are initialized
      const protocolAccountInfo = await program.account.protocol.fetch(protocolAccount.publicKey);
      const userAccountInfo = await program.account.userInfo.fetch(userInfo.publicKey);
      
      if (!protocolAccountInfo || !userAccountInfo) {
        throw new Error("Failed to initialize accounts");
      }
      
      console.log("Protocol account info:", protocolAccountInfo);
      console.log("User info account info:", userAccountInfo);
      
      isInitialized = true;
    } catch (e) {
      console.error("Initialization failed with error:", e);
      if ('logs' in e) {
        console.error("Transaction logs:", e.logs);
      }
      throw e;
    }
  });

  // Make other tests dependent on initialization
  describe("After initialization", () => {
    beforeEach(async () => {
      if (!isInitialized) {
        throw new Error("Protocol not initialized");
      }
    });

    it("Gets pool length", async () => {
      console.log("Fetching pool accounts...");
      // First verify the program account exists
      const programAcct = await provider.connection.getAccountInfo(program.programId);
      if (!programAcct) {
        throw new Error("Program account not found");
      }

      try {
        const poolAccounts = await program.account.pool.all();
        console.log("Pool length:", poolAccounts.length);
      } catch (e) {
        console.error("Failed to fetch pool accounts:", e);
        throw e;
      }
    });

    it("Fetches deposits pool length", async () => {
      const pid = new anchor.BN(0);
      try {
        const depositsPoolLength = await program.methods.depositsPoolLength(pid)
          .accounts({
            protocol: protocolAccount.publicKey,
            userInfo: userInfo.publicKey,
            pool: protocolAccount.publicKey,
          })
          .view(); // Using view instead of rpc for read-only operations
        console.log("Deposits pool length:", depositsPoolLength);
      } catch (e) {
        console.error("Failed to fetch deposits pool length:", e);
        throw e;
      }
    });

    it("Fetches available sum for withdraw", async () => {
      const pid = new anchor.BN(0);
      try {
        const availableSum = await program.methods.getAvailableSumForWithdraw(pid)
          .accounts({
            protocol: protocolAccount.publicKey,
            userInfo: userInfo.publicKey,
            pool: protocolAccount.publicKey, // placeholder
          })
          .view(); // Using view instead of rpc for read-only operations
        console.log("Available sum for withdraw:", availableSum);
      } catch (e) {
        console.error("Failed to fetch available sum for withdraw:", e);
        throw e;
      }
    });

    it("Fetches claimable amount", async () => {
      const pid = new anchor.BN(0);
      try {
        const claimable = await program.methods.getClaimable(pid)
          .accounts({
            protocol: protocolAccount.publicKey,
            userInfo: userInfo.publicKey,
            pool: protocolAccount.publicKey, // placeholder
          })
          .view(); // Using view instead of rpc for read-only operations
        console.log("Claimable amount:", claimable);
      } catch (e) {
        console.error("Failed to fetch claimable amount:", e);
        throw e;
      }
    });

    it("Fetches pool rate and APY", async () => {
      const pid = new anchor.BN(0);
      const timestamp = new anchor.BN(Math.floor(Date.now() / 1000));
      try {
        const poolRateAndApy = await program.methods.getPoolRateAndApy(pid, timestamp)
          .accounts({
            protocol: protocolAccount.publicKey,
            pool: protocolAccount.publicKey, // placeholder
          })
          .view(); // Using view instead of rpc for read-only operations
        console.log("Pool rate and APY:", poolRateAndApy);
      } catch (e) {
        console.error("Failed to fetch pool rate and APY:", e);
        throw e;
      }
    });

    it("Gets pool accounts", async () => {
      console.log("Fetching pool accounts...");
      try {
        const poolAccounts = await program.account.pool.all();  // Fetch all pool accounts
        console.log("Pool accounts:", poolAccounts);
      } catch (e) {
        console.error("Failed to fetch pool accounts:", e);
        throw e;
      }
    });
  });
});
