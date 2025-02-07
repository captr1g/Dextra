import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Freelance } from "../target/types/freelance";
import { TOKEN_PROGRAM_ID, createMint, createAssociatedTokenAccount } from "@solana/spl-token";

describe("manual calls", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.Freelance as Program<Freelance>;

  // Store accounts that will be reused across tests
  const protocolAccount = anchor.web3.Keypair.generate();
  const poolAccount = anchor.web3.Keypair.generate();
  let dummyPool: anchor.web3.Keypair;
  const userInfoAccount = anchor.web3.Keypair.generate();
  const provider = anchor.getProvider();
  const wallet = (provider as anchor.AnchorProvider).wallet;

  before(async () => {
    // Airdrop to the wallet
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(wallet.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      "confirmed"
    );
  });

  it("Manual call to initialize", async () => {
    const tx = await program.methods
      .initialize()
      .accounts({
        protocol: protocolAccount.publicKey,
        userInfo: userInfoAccount.publicKey,
        owner: wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([protocolAccount, userInfoAccount])
      .rpc();

    console.log("Initialize transaction signature:", tx);
  });

  it("Manual call to add_pool", async () => {
    dummyPool = anchor.web3.Keypair.generate();
    const depositToken = anchor.web3.Keypair.generate();
    const rewardToken = anchor.web3.Keypair.generate();

    // Then add pool
    const tx = await program.methods
      .addPool(
        new anchor.BN(1000),
        new anchor.BN(3600),
        true,
        new anchor.BN(500),
        new anchor.BN(100)
      )
      .accounts({
        protocol: protocolAccount.publicKey,
        pool: dummyPool.publicKey,
        depositToken: depositToken.publicKey,
        rewardToken: rewardToken.publicKey,
        payer: wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([dummyPool]) // Ensure dummyPool is included as a signer
      .rpc();

    console.log("Add pool tx:", tx);

    // Wait for confirmation
    await new Promise((resolve) => setTimeout(resolve, 2000));
  });

  it("Manual call to deposit", async () => {
    // Create token accounts first
    const mint = await createMint(
      provider.connection,
      wallet.payer,
      wallet.publicKey,
      null,
      9,
      undefined,
      TOKEN_PROGRAM_ID
    );

    const userTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      mint,
      wallet.publicKey,
      undefined,
      TOKEN_PROGRAM_ID
    );

    const protocolTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      mint,
      protocolAccount.publicKey,
      undefined,
      TOKEN_PROGRAM_ID
    );

    const tx = await program.methods
      .deposit(
        new anchor.BN(1),
        new anchor.BN(2000),
        null
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
      .rpc();

    console.log("Deposit transaction signature:", tx);
  });

  it("Manual call to claim", async () => {
    // Wait for previous transactions to confirm
    await new Promise((resolve) => setTimeout(resolve, 1000));

    const protocolVault = await anchor.utils.token.associatedAddress({
      mint: dummyPool.publicKey,
      owner: protocolAccount.publicKey,
    });

    const referrerVault = anchor.web3.Keypair.generate().publicKey;

    const tx = await program.methods
      .claim(new anchor.BN(1))
      .accounts({
        pool: dummyPool.publicKey,
        protocol: protocolAccount.publicKey,
        userInfo: userInfoAccount.publicKey,
        protocolVault: protocolVault,
        referrerVault: referrerVault,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Claim transaction signature:", tx);
  });

  it("should add a new pool", async () => {
    dummyPool = anchor.web3.Keypair.generate(); // Generate dummyPool only once
    // Dummy public keys for tokens, replace with valid ones if available
    const depositToken = anchor.web3.Keypair.generate().publicKey;
    const rewardToken = anchor.web3.Keypair.generate().publicKey;

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
      .signers([dummyPool]) // Ensure dummyPool is included as a signer
      .rpc();
    console.log("Pool added:", dummyPool.publicKey.toString());
  });
});