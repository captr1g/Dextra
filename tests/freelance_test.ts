import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Freelance } from "../target/types/freelance";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

// Set custom provider
anchor.setProvider(anchor.AnchorProvider.env());

const program = anchor.workspace.Freelance as Program<Freelance>;

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

describe("freelance", () => {
  // Static accounts, generated only once
  const protocolAccount = anchor.web3.Keypair.generate();
  const userInfoAccount = anchor.web3.Keypair.generate();
  let dummyPool: anchor.web3.Keypair; // Declare dummyPool
  let protocolVault: anchor.web3.Keypair;
  let referrerVault: anchor.web3.Keypair;

  const provider = anchor.AnchorProvider.env();
  const wallet = (provider as anchor.AnchorProvider).wallet as anchor.Wallet;

  before(async () => {
    try {
      // Airdrop to the wallet
      await provider.connection.confirmTransaction(
        await provider.connection.requestAirdrop(wallet.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
        "confirmed"
      );

      // Initialize protocol and user info accounts
      await program.methods
        .initialize()
        .accounts({
          protocol: protocolAccount.publicKey, // Ensure this matches the expected account names in the claim method
          userInfo: userInfoAccount.publicKey,
          owner: wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([protocolAccount, userInfoAccount])
        .rpc();

      console.log("Protocol account pubkey:", protocolAccount.publicKey.toString());
      console.log("User info pubkey:", userInfoAccount.publicKey.toString());

      const latestBlockhash = await provider.connection.getLatestBlockhash();
      console.log("Connected to validator, blockhash:", latestBlockhash.blockhash);
    } catch (e) {
      console.error("Failed to connect to validator:", e);
      throw e;
    }
  });

  beforeEach(async () => {
    dummyPool = anchor.web3.Keypair.generate(); // Generate dummyPool before each test
    protocolVault = anchor.web3.Keypair.generate();
    referrerVault = anchor.web3.Keypair.generate();

    // Airdrop SOL to protocolVault with retry logic
    let airdropSuccess = false;
    let retryCount = 0;
    while (!airdropSuccess && retryCount < 3) {
      try {
        await provider.connection.confirmTransaction(
          await provider.connection.requestAirdrop(protocolVault.publicKey, anchor.web3.LAMPORTS_PER_SOL),
          "confirmed"
        );
        airdropSuccess = true;
      } catch (error) {
        console.warn(`Airdrop to protocolVault failed (attempt ${retryCount + 1}):`, error);
        retryCount++;
        await sleep(1000); // Wait before retrying
      }
    }

    airdropSuccess = false;
    retryCount = 0;
    while (!airdropSuccess && retryCount < 3) {
      try {
        await provider.connection.confirmTransaction(
          await provider.connection.requestAirdrop(referrerVault.publicKey, anchor.web3.LAMPORTS_PER_SOL),
          "confirmed"
        );
        airdropSuccess = true;
      } catch (error) {
        console.warn(`Airdrop to referrerVault failed (attempt ${retryCount + 1}):`, error);
        retryCount++;
        await sleep(1000); // Wait before retrying
      }
    }

    // Initialize the pool account
    const depositTokenKeypair = anchor.web3.Keypair.generate();
    const rewardTokenKeypair = anchor.web3.Keypair.generate();
    const depositToken = depositTokenKeypair.publicKey;
    const rewardToken = rewardTokenKeypair.publicKey;

    try {
      await program.methods
        .addPool(
          new anchor.BN(1000), // minimumDeposit
          new anchor.BN(3600), // lockPeriod
          true, // canSwap
          new anchor.BN(500), // rate
          new anchor.BN(100) // apy
        )
        .accounts({
          protocol: protocolAccount.publicKey,
          pool: dummyPool.publicKey,
          depositToken: depositToken,
          rewardToken: rewardToken,
          payer: wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([dummyPool, depositTokenKeypair, rewardTokenKeypair]) // Ensure dummyPool and wallet.payer are included as signers
        .rpc();
      console.log("Pool added:", dummyPool.publicKey.toString());
    } catch (error) {
      console.error("addPool instruction failed:", error);
      throw error; // Re-throw the error to fail the test
    }
  });

  it("should initialize protocol and user info", async () => {
    const newProtocol = anchor.web3.Keypair.generate();
    const newUserInfo = anchor.web3.Keypair.generate();
    const provider = anchor.getProvider();
    const wallet = provider.wallet as anchor.Wallet;

    await program.methods
      .initialize()
      .accounts({
        protocol: newProtocol.publicKey,
        userInfo: newUserInfo.publicKey,
        owner: wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([newProtocol, newUserInfo])
      .rpc();

    const accountData = await program.account.userInfo.fetch(newUserInfo.publicKey);
    console.log("Initialized userInfo account:", accountData);
    if (!accountData) {
      throw new Error("UserInfo account not initialized");
    }
  });

  it("should add a new pool", async () => {
    // Dummy public keys for tokens, replace with valid ones if available
    const depositTokenKeypair = anchor.web3.Keypair.generate();
    const rewardTokenKeypair = anchor.web3.Keypair.generate();
    const depositToken = depositTokenKeypair.publicKey;
    const rewardToken = rewardTokenKeypair.publicKey;

    await program.methods
      .addPool(
        new anchor.BN(1000), // minimumDeposit
        new anchor.BN(3600), // lockPeriod
        true, // canSwap
        new anchor.BN(500), // rate
        new anchor.BN(100) // apy
      )
      .accounts({
        protocol: protocolAccount.publicKey,
        pool: dummyPool.publicKey,
        depositToken: depositToken,
        rewardToken: rewardToken,
        payer: wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([dummyPool, depositTokenKeypair, rewardTokenKeypair]) // Ensure dummyPool is included as a signer
      .rpc();
    console.log("Pool added:", dummyPool.publicKey.toString());
  });

  it("should approve a user", async () => {
    const dummyUser = anchor.web3.Keypair.generate().publicKey;
    await program.methods.approve(
      dummyUser,
      1 // dummy approvalType
    )
    .accounts({
      protocol: protocolAccount.publicKey,
      authority: wallet.publicKey,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();
    console.log("User approved:", dummyUser.toString());
  });

  it("should deposit funds", async () => {
    // Dummy token accounts; in a real test these should be valid token accounts.
    const userTokenAccountKeypair = anchor.web3.Keypair.generate();
    const protocolTokenAccountKeypair = anchor.web3.Keypair.generate();
    const userTokenAccount = userTokenAccountKeypair.publicKey;
    const protocolTokenAccount = protocolTokenAccountKeypair.publicKey;

    await program.methods
      .deposit(
        new anchor.BN(1), // poolId
        new anchor.BN(2000), // amount
        null // referrer (null for no referrer)
      )
      .accounts({
        pool: dummyPool.publicKey,
        userInfo: userInfoAccount.publicKey,
        protocol: protocolAccount.publicKey,
        userTokenAccount: userTokenAccount,
        protocolTokenAccount: protocolTokenAccount,
        user: wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([userInfoAccount, userTokenAccountKeypair]) // Add userInfoAccount to signers
      .rpc();
    console.log("Deposit executed on pool:", dummyPool.publicKey.toString());
  });

  it("should claim rewards", async () => {
    // Now execute the claim instruction
    await program.methods
      .claim(
        new anchor.BN(1) // poolId
      )
      .accounts({
        pool: dummyPool.publicKey,
        protocol: protocolAccount.publicKey,
        userInfo: userInfoAccount.publicKey,
        protocolVault: protocolVault.publicKey,
        referrerVault: referrerVault.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    console.log("Claim executed for pool:", dummyPool.publicKey.toString());
  });

});
