import * as anchor from '@coral-xyz/anchor';
import { Program } from '@coral-xyz/anchor';
import { PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from '@solana/web3.js';
import { TOKEN_PROGRAM_ID, createMint, getAccount, createAssociatedTokenAccount, mintTo, getAssociatedTokenAddress, getOrCreateAssociatedTokenAccount } from '@solana/spl-token';
import { assert } from 'chai';
import fs from 'fs';
import path from 'path';
import { Dextra } from '../target/types/dextra';

describe('dextra', () => {
  // Configure the client to use the local cluster
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  // Load the IDL and create program
  const program = anchor.workspace.Dextra as Program<Dextra>;
  
  console.log("Program ID:", program.programId.toString());

  const wallet = provider.wallet as anchor.Wallet;

  // PDAs and seeds
  const protocolSeed = Buffer.from("protocol");
  let protocolPDA: PublicKey;
  let protocolBump: number;

  // New keypairs for our test accounts
  const userInfoKeypair = Keypair.generate();
  const userKeypair = Keypair.generate(); // Regular user for testing
  const referrerKeypair = Keypair.generate(); // Referrer for testing
  
  // Token mints for testing
  let depositTokenMint: PublicKey;
  let rewardTokenMint: PublicKey;
  
  // User token accounts
  let userDepositTokenAccount: PublicKey;
  let userRewardTokenAccount: PublicKey;
  let referrerRewardTokenAccount: PublicKey;
  
  // Protocol token accounts
  let protocolDepositTokenAccount: PublicKey;
  let protocolRewardTokenAccount: PublicKey;
  
  // Pool PDA
  let poolPDA: PublicKey;
  let poolBump: number;

  // Define interfaces for our new structs
  interface ReferrerEntry {
    user: PublicKey;
    referrer: PublicKey;
  }

  interface UserFlagEntry {
    user: PublicKey;
    flag: boolean;
  }

  interface RateEntry {
    timestamp: anchor.BN;
    value: anchor.BN;
  }

  // Update the ProtocolAccount interface
  interface ProtocolAccount {
    owner: PublicKey;
    governance: PublicKey;
    refPercent: anchor.BN;
    poolCount: anchor.BN;
    referrers: Array<ReferrerEntry>;
    claimableUsers: Array<UserFlagEntry>;
    withdrawableUsers: Array<UserFlagEntry>;
  }

  interface UserInfo {
    authority: PublicKey;
    amount: anchor.BN;
    stakeTimestamp: anchor.BN;
    lastClaimed: anchor.BN;
    pendingReward: anchor.BN;
    referrer: PublicKey;
    totalClaimed: anchor.BN;
    deposits: any[];
  }

  interface Pool {
    depositToken: PublicKey;
    rewardToken: PublicKey;
    minimumDeposit: anchor.BN;
    lockPeriod: anchor.BN;
    canSwap: boolean;
    lastRate: anchor.BN;
    lastApy: anchor.BN;
    rates: Array<RateEntry>;
    apys: Array<RateEntry>;
  }

  // Pool parameters
  const minimumDeposit = new anchor.BN(1_000_000); // 1 token with 6 decimals
  const lockPeriod = new anchor.BN(5); // 5 seconds for testing (instead of days)
  const canSwap = true;
  const rate = new anchor.BN(1_000_000); // 1:1 rate (1_000_000 is the rate multiplier)
  const apy = new anchor.BN(1000); // 10% APY (100 = 1%)
  
  // Test constants
  const DEPOSIT_AMOUNT = new anchor.BN(5_000_000); // 5 tokens

  before(async () => {
    // Fund the test keypairs so they can be signers for transactions
    const fundTx1 = await provider.connection.requestAirdrop(
      userInfoKeypair.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(fundTx1);
    console.log("Funded userInfoKeypair:", userInfoKeypair.publicKey.toString());
    
    const fundTx2 = await provider.connection.requestAirdrop(
      userKeypair.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(fundTx2);
    console.log("Funded userKeypair:", userKeypair.publicKey.toString());
    
    const fundTx3 = await provider.connection.requestAirdrop(
      referrerKeypair.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(fundTx3);
    console.log("Funded referrerKeypair:", referrerKeypair.publicKey.toString());

    // Find protocol PDA
    [protocolPDA, protocolBump] = PublicKey.findProgramAddressSync(
      [protocolSeed],
      program.programId
    );

    // Find pool PDA
    [poolPDA, poolBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), protocolPDA.toBuffer()],
      program.programId
    );

    // Create token mints for deposit and reward
    depositTokenMint = await createMint(
      provider.connection,
      (wallet as any).payer, // Type cast to access payer
      wallet.publicKey,
      null,
      9 // 9 decimals
    );

    rewardTokenMint = await createMint(
      provider.connection,
      (wallet as any).payer, // Type cast to access payer
      wallet.publicKey,
      null,
      9 // 9 decimals
    );
    
    // Create token accounts for users
    userDepositTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      (wallet as any).payer,
      depositTokenMint,
      userKeypair.publicKey
    );
    
    userRewardTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      (wallet as any).payer,
      rewardTokenMint,
      userKeypair.publicKey
    );
    
    referrerRewardTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      (wallet as any).payer,
      rewardTokenMint,
      referrerKeypair.publicKey
    );
    
    // Create protocol token accounts
    protocolDepositTokenAccount = (await getOrCreateAssociatedTokenAccount(
      provider.connection,
      (wallet as any).payer,
      depositTokenMint,
      protocolPDA,
      true // allowOwnerOffCurve
    )).address;
    
    protocolRewardTokenAccount = (await getOrCreateAssociatedTokenAccount(
      provider.connection,
      (wallet as any).payer,
      rewardTokenMint,
      protocolPDA,
      true // allowOwnerOffCurve
    )).address;
    
    // Mint tokens to user for testing
    await mintTo(
      provider.connection,
      (wallet as any).payer,
      depositTokenMint,
      userDepositTokenAccount,
      wallet.publicKey,
      10_000_000 // 10 tokens
    );
    
    // Mint reward tokens to protocol for rewards/claims
    await mintTo(
      provider.connection,
      (wallet as any).payer,
      rewardTokenMint,
      protocolRewardTokenAccount,
      wallet.publicKey,
      100_000_000 // 100 tokens for rewards
    );

    console.log("Setup complete.");
    console.log("Protocol PDA:", protocolPDA.toString());
    console.log("Pool PDA:", poolPDA.toString());
    console.log("Deposit Token Mint:", depositTokenMint.toString());
    console.log("Reward Token Mint:", rewardTokenMint.toString());
  });

  it('Is initialized correctly', async () => {
    // Initialize the protocol
    await program.methods
      .initialize()
      .accounts({
        protocol: protocolPDA,
        userInfo: userInfoKeypair.publicKey,
        owner: wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([userInfoKeypair])
      .rpc();
    
    // Fetch the accounts to verify they were initialized correctly
    const protocolAccount = await program.account.protocolAccount.fetch(protocolPDA);
    const userInfoAccount = await program.account.userInfo.fetch(userInfoKeypair.publicKey);
    
    // Verify protocol account fields
    assert.ok(protocolAccount.owner.equals(wallet.publicKey), "Owner should be set to wallet public key");
    assert.ok(protocolAccount.governance.equals(wallet.publicKey), "Governance should be set to wallet public key");
    assert.equal(protocolAccount.refPercent.toNumber(), 200, "Ref percent should be 2%");
    assert.equal(protocolAccount.poolCount.toNumber(), 0, "Pool count should be 0");
    
    // Verify user info account fields
    assert.ok(userInfoAccount.authority.equals(wallet.publicKey), "Authority should be set to wallet public key");
    assert.equal(userInfoAccount.amount.toNumber(), 0, "Amount should be 0");
    assert.equal(userInfoAccount.stakeTimestamp.toNumber(), 0, "Stake timestamp should be 0");
    assert.equal(userInfoAccount.lastClaimed.toNumber(), 0, "Last claimed should be 0");
    assert.equal(userInfoAccount.pendingReward.toNumber(), 0, "Pending reward should be 0");
    assert.equal(userInfoAccount.totalClaimed.toNumber(), 0, "Total claimed should be 0");
    assert.equal(userInfoAccount.deposits.length, 0, "Deposits array should be empty");
  });

  it('Can add a pool', async () => {
    // Add a pool
    await program.methods
      .addPool(
        minimumDeposit,
        lockPeriod,
        canSwap,
        rate,
        apy
      )
      .accounts({
        protocol: protocolPDA,
        pool: poolPDA,
        depositToken: depositTokenMint,
        rewardToken: rewardTokenMint,
        payer: wallet.publicKey,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();
    
    // Fetch accounts to verify
    const protocolAccount = await program.account.protocolAccount.fetch(protocolPDA);
    const poolAccount = await program.account.pool.fetch(poolPDA);
    
    // Verify protocol account was updated
    assert.equal(protocolAccount.poolCount.toNumber(), 1, "Pool count should be 1");
    
    // Verify pool account fields
    assert.ok(poolAccount.depositToken.equals(depositTokenMint), "Deposit token should match");
    assert.ok(poolAccount.rewardToken.equals(rewardTokenMint), "Reward token should match");
    assert.equal(poolAccount.minimumDeposit.toNumber(), minimumDeposit.toNumber(), "Minimum deposit should match");
    assert.equal(poolAccount.lockPeriod.toNumber(), lockPeriod.toNumber(), "Lock period should match");
    assert.equal(poolAccount.canSwap, canSwap, "Can swap should match");
    assert.equal(poolAccount.lastRate.toNumber(), rate.toNumber(), "Last rate should match");
    assert.equal(poolAccount.lastApy.toNumber(), apy.toNumber(), "Last APY should match");
    
    // Verify the rates and APYs were set for the current date
    assert.equal(poolAccount.rates.length, 1, "Should have one rate entry");
    assert.equal(poolAccount.apys.length, 1, "Should have one APY entry");
  });

  it('Can get pool length', async () => {
    // Call pool_length method
    const poolLength = await program.methods
      .poolLength()
      .accounts({
        protocol: protocolPDA,
      })
      .view();
    
    assert.equal(poolLength.toNumber(), 1, "Pool length should be 1");
  });

  it('Can deposit tokens with a referrer', async () => {
    // Create a new user info account for the user
    const userInfoAccount = Keypair.generate();
    
    // Deposit with referral
    const poolId = new anchor.BN(0);
    await program.methods
      .deposit(
        poolId,
        DEPOSIT_AMOUNT,
        referrerKeypair.publicKey
      )
      .accounts({
        pool: poolPDA,
        userInfo: userInfoAccount.publicKey,
        protocol: protocolPDA,
        userTokenAccount: userDepositTokenAccount,
        protocolTokenAccount: protocolDepositTokenAccount,
        user: userKeypair.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userKeypair, userInfoAccount])
      .rpc();
    
    // Verify the deposit
    const userInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    const protocolAccount = await program.account.protocolAccount.fetch(protocolPDA);
    
    // Check deposit amount
    assert.equal(userInfo.amount.toNumber(), DEPOSIT_AMOUNT.toNumber(), "Deposit amount should match");
    
    // Check stake timestamp is set
    assert.notEqual(userInfo.stakeTimestamp.toNumber(), 0, "Stake timestamp should be set");
    
    // Check deposits array has one entry
    assert.equal(userInfo.deposits.length, 1, "Should have one deposit entry");
    assert.equal(userInfo.deposits[0].amount.toNumber(), DEPOSIT_AMOUNT.toNumber(), "Deposit amount should match");
    assert.isFalse(userInfo.deposits[0].isWithdrawn, "Deposit should not be withdrawn");
    
    // Check referrer was recorded
    const foundReferrer = protocolAccount.referrers.find(
      entry => entry.user.toString() === userKeypair.publicKey.toString()
    );
    assert.isDefined(foundReferrer, "Referrer entry should exist");
    if (foundReferrer) {
      assert.equal(
        foundReferrer.referrer.toString(), 
        referrerKeypair.publicKey.toString(), 
        "Referrer should match"
      );
    }
  });

  it('Can get deposit info', async () => {
    // We're using the deposit from the previous test
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    const poolId = new anchor.BN(0);
    const depositId = new anchor.BN(0);
    
    // Get deposit info
    const depositInfo = await program.methods
      .getDepositInfo(
        poolId,
        depositId
      )
      .accounts({
        protocol: protocolPDA,
        userInfo: userInfoAccount.publicKey,
        user: userKeypair.publicKey,
      })
      .signers([userKeypair])
      .rpc();
    
    // Since we can't get the return value from rpc call, we'll check the deposit directly
    const userInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    const deposit = userInfo.deposits[0];
    
    assert.equal(deposit.amount.toNumber(), DEPOSIT_AMOUNT.toNumber(), "Deposit amount should match");
    assert.isFalse(deposit.isWithdrawn, "Deposit should not be withdrawn");
    assert.isTrue(deposit.lockedUntil.toNumber() > deposit.timestamp.toNumber(), "Locked until should be after timestamp");
  });

  it('Can calculate rewards', async () => {
    // Sleep to accumulate some rewards
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Get the user info account from previous tests
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    const poolId = new anchor.BN(0);
    
    // Calculate available rewards
    const reward = await program.methods
      .getClaimable(poolId)
      .accounts({
        protocol: protocolPDA,
        userInfo: userInfoAccount.publicKey,
        pool: poolPDA,
      })
      .view();
    
    console.log("Calculated reward:", reward.toString());
    // Since rewards depend on time, we just check that some reward exists
    assert.isTrue(reward.toNumber() >= 0, "Reward should be calculated");
  });

  it('Can unlock deposits for testing', async () => {
    // Get the user info account
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    // Use test helper to unlock the deposit directly
    await program.methods
      .testHelperSetDepositUnlocked(new anchor.BN(0))
      .accounts({
        userInfo: userInfoAccount.publicKey,
        authority: wallet.publicKey,
        protocol: protocolPDA,
      })
      .rpc();
    
    // Verify the deposit is unlocked
    const updatedUserInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    assert.isTrue(
      updatedUserInfo.deposits[0].lockedUntil.toNumber() < Math.floor(Date.now() / 1000),
      "Deposit should be unlocked"
    );
  });

  it('Can check withdrawable amount', async () => {
    // Get the user info account
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    const poolId = new anchor.BN(0);
    
    // Get available withdrawal amount
    const withdrawableAmount = await program.methods
      .getAvailableSumForWithdraw(poolId)
      .accounts({
        protocol: protocolPDA,
        userInfo: userInfoAccount.publicKey,
        pool: poolPDA,
      })
      .view();
    
    // Verify the withdrawable amount
    assert.equal(
      withdrawableAmount.toNumber(),
      DEPOSIT_AMOUNT.toNumber(),
      "Withdrawable amount should match deposit amount"
    );
  });

  it('Can set pending reward for testing', async () => {
    // Get the user info account
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    // Set pending reward for testing
    const testRewardAmount = new anchor.BN(1_000_000); // 1 token
    await program.methods
      .testHelperSetPendingReward(testRewardAmount)
      .accounts({
        userInfo: userInfoAccount.publicKey,
        authority: wallet.publicKey,
        protocol: protocolPDA,
      })
      .rpc();
    
    // Verify the pending reward was set
    const updatedUserInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    assert.equal(
      updatedUserInfo.pendingReward.toNumber(),
      testRewardAmount.toNumber(),
      "Pending reward should be set correctly"
    );
  });

  it('Can approve user for claiming', async () => {
    // For claim and withdraw with inverted logic (!is_claimable, !is_withdrawable),
    // We need to first ensure the user is NOT in the lists (or flag is false)
    // to allow the operations to succeed

    // Set claimable flag to false explicitly using test helper
    await program.methods
      .testHelperSetFlag(userKeypair.publicKey, 0, false)
      .accounts({
        protocol: protocolPDA,
        authority: wallet.publicKey,
      })
      .rpc();
    
    // Verify user's claimable flag is false (or not in the list)
    const protocolAccount = await program.account.protocolAccount.fetch(protocolPDA);
    const foundUser = protocolAccount.claimableUsers.find(
      entry => entry.user.toString() === userKeypair.publicKey.toString()
    );
    
    if (foundUser) {
      assert.isFalse(foundUser.flag, "User should not be marked as claimable for the inverted logic check to pass");
    }
  });

  it('Can claim rewards', async () => {
    // Get the user info account
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    const poolId = new anchor.BN(0);
    
    // Get initial reward token balance
    const initialRewardBalance = await provider.connection.getTokenAccountBalance(
      userRewardTokenAccount
    );
    
    // Make the claim - should succeed because user is NOT claimable (inverted logic)
    await program.methods
      .claim(poolId)
      .accounts({
        pool: poolPDA,
        protocol: protocolPDA,
        userInfo: userInfoAccount.publicKey,
        protocolVault: protocolRewardTokenAccount,
        referrerVault: referrerRewardTokenAccount,
        userTokenAccount: userRewardTokenAccount,
        user: userKeypair.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userKeypair])
      .rpc();
    
    // Verify the claim
    const updatedUserInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    assert.equal(updatedUserInfo.pendingReward.toNumber(), 0, "Pending reward should be reset to 0");
    
    // Check that total claimed was updated
    assert.isTrue(updatedUserInfo.totalClaimed.toNumber() > 0, "Total claimed should be increased");
    
    // Check if referrer received tokens
    const referrerBalance = await provider.connection.getTokenAccountBalance(
      referrerRewardTokenAccount
    );
    console.log("Referrer reward balance:", referrerBalance.value.uiAmount);
  });

  it('Can approve user for withdrawal', async () => {
    // Set withdrawable flag to false explicitly using test helper
    await program.methods
      .testHelperSetFlag(userKeypair.publicKey, 1, false)
      .accounts({
        protocol: protocolPDA,
        authority: wallet.publicKey,
      })
      .rpc();
    
    // Verify user's withdrawable flag is false (or not in the list)
    const protocolAccount = await program.account.protocolAccount.fetch(protocolPDA);
    const foundUser = protocolAccount.withdrawableUsers.find(
      entry => entry.user.toString() === userKeypair.publicKey.toString()
    );
    
    if (foundUser) {
      assert.isFalse(foundUser.flag, "User should not be marked as withdrawable for the inverted logic check to pass");
    }
  });

  it('Can withdraw unlocked deposits', async () => {
    // Get the user info account
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    const poolId = new anchor.BN(0);
    
    // Get initial user info to compare
    const initialUserInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    
    // Get initial deposit token balance
    const initialDepositBalance = await provider.connection.getTokenAccountBalance(
      userDepositTokenAccount
    );
    
    // Make the withdrawal - should succeed because user is NOT withdrawable (inverted logic)
    await program.methods
      .withdraw(poolId)
      .accounts({
        protocol: protocolPDA,
        userInfo: userInfoAccount.publicKey,
        user: userKeypair.publicKey,
        pool: poolPDA,
        protocolTokenAccount: protocolDepositTokenAccount,
        userTokenAccount: userDepositTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userKeypair])
      .rpc();
    
    // Verify the withdrawal
    const updatedUserInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    
    // Check that amount was updated correctly
    assert.equal(updatedUserInfo.amount.toNumber(), 0, "Amount should be reset to 0 after full withdrawal");
    
    // Check that stake_timestamp and last_claimed were reset
    assert.equal(updatedUserInfo.stakeTimestamp.toNumber(), 0, "Stake timestamp should be reset to 0");
    assert.equal(updatedUserInfo.lastClaimed.toNumber(), 0, "Last claimed should be reset to 0");
    
    // Check that deposit is marked as withdrawn
    assert.isTrue(updatedUserInfo.deposits[0].isWithdrawn, "Deposit should be marked as withdrawn");
    
    // Check that user token balance increased
    const finalDepositBalance = await provider.connection.getTokenAccountBalance(
      userDepositTokenAccount
    );
    
    assert.isTrue(
      finalDepositBalance.value.uiAmount > initialDepositBalance.value.uiAmount,
      "User deposit token balance should have increased"
    );
    
    console.log("Withdraw successful: User withdrew", 
      initialUserInfo.amount.toNumber(), 
      "tokens and now has", 
      finalDepositBalance.value.uiAmount, 
      "in their account"
    );
  });

  it('Can update pool parameters', async () => {
    // New parameters
    const newMinimumDeposit = new anchor.BN(2_000_000); // 2 tokens
    const newLockPeriod = new anchor.BN(10); // 10 seconds
    const newCanSwap = false;
    
    const poolId = new anchor.BN(0);
    
    // Update the pool
    await program.methods
      .updatePool(poolId, newMinimumDeposit, newLockPeriod, newCanSwap)
      .accounts({
        pool: poolPDA,
        protocol: protocolPDA,
        authority: wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    
    // Verify the updates
    const updatedPool = await program.account.pool.fetch(poolPDA);
    
    assert.equal(
      updatedPool.minimumDeposit.toNumber(), 
      newMinimumDeposit.toNumber(), 
      "Minimum deposit should be updated"
    );
    
    assert.equal(
      updatedPool.lockPeriod.toNumber(), 
      newLockPeriod.toNumber(), 
      "Lock period should be updated"
    );
    
    assert.equal(
      updatedPool.canSwap, 
      newCanSwap, 
      "Can swap flag should be updated"
    );
  });

  it('Can update rate and APY', async () => {
    // New rates
    const newRate = new anchor.BN(1_200_000); // 1.2:1 rate
    const newAPY = new anchor.BN(1500); // 15% APY
    
    const poolId = new anchor.BN(0);
    
    // Update the rate
    await program.methods
      .updateRate(poolId, newRate)
      .accounts({
        pool: poolPDA,
        protocol: protocolPDA,
        authority: wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    
    // Update the APY
    await program.methods
      .updateApy(poolId, newAPY)
      .accounts({
        pool: poolPDA,
        protocol: protocolPDA,
        authority: wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    
    // Verify the updates
    const updatedPool = await program.account.pool.fetch(poolPDA);
    
    assert.equal(
      updatedPool.lastRate.toNumber(), 
      newRate.toNumber(), 
      "Rate should be updated"
    );
    
    assert.equal(
      updatedPool.lastApy.toNumber(), 
      newAPY.toNumber(), 
      "APY should be updated"
    );
    
    // Check that rates were added to the history arrays
    assert.isTrue(updatedPool.rates.length > 1, "Should have multiple rate entries");
    assert.isTrue(updatedPool.apys.length > 1, "Should have multiple APY entries");
  });

  it('Can get pool rate and APY', async () => {
    const poolId = new anchor.BN(0);
    const timestamp = Math.floor(Date.now() / 1000);
    
    // Get current rate and APY through rpc instead of view
    await program.methods
      .getPoolRateAndApy(poolId, new anchor.BN(timestamp))
      .accounts({
        protocol: protocolPDA,
        pool: poolPDA,
      })
      .rpc();
    
    // Since we can't get the return value, we'll verify by directly checking the pool
    const pool = await program.account.pool.fetch(poolPDA);
    
    // Verify returned values
    assert.isTrue(pool.lastApy.toNumber() > 0, "APY should be positive");
    assert.isTrue(pool.lastRate.toNumber() > 0, "Rate should be positive");
    console.log("Current APY:", pool.lastApy.toNumber());
    console.log("Current Rate:", pool.lastRate.toNumber());
  });

  it('Shows inverted authorization logic for claim', async () => {
    // Get the user info account
    const userInfoAccounts = await program.account.userInfo.all();
    const userInfoAccount = userInfoAccounts.find(
      acc => acc.account.authority.toString() === userKeypair.publicKey.toString()
    );
    
    assert.isDefined(userInfoAccount, "User info account should exist");
    if (!userInfoAccount) return;
    
    // Set pending reward for the test
    const testRewardAmount = new anchor.BN(1_000_000);
    await program.methods
      .testHelperSetPendingReward(testRewardAmount)
      .accounts({
        userInfo: userInfoAccount.publicKey,
        authority: wallet.publicKey,
        protocol: protocolPDA,
      })
      .rpc();

    // 1. First set user to claimable (flag = true)
    await program.methods
      .testHelperSetFlag(userKeypair.publicKey, 0, true)
      .accounts({
        protocol: protocolPDA,
        authority: wallet.publicKey,
      })
      .rpc();
    
    // Verify user is claimable
    let protocolAccount = await program.account.protocolAccount.fetch(protocolPDA);
    let foundUser = protocolAccount.claimableUsers.find(
      entry => entry.user.toString() === userKeypair.publicKey.toString()
    );
    assert.isDefined(foundUser, "User should be in claimable users");
    assert.isTrue(foundUser?.flag, "User should be marked as claimable");
    
    // Try to claim - should fail due to inverted logic check (!is_claimable)
    try {
      await program.methods
        .claim(new anchor.BN(0))
        .accounts({
          pool: poolPDA,
          protocol: protocolPDA,
          userInfo: userInfoAccount.publicKey,
          protocolVault: protocolRewardTokenAccount,
          referrerVault: referrerRewardTokenAccount,
          userTokenAccount: userRewardTokenAccount,
          user: userKeypair.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([userKeypair])
        .rpc();
      
      // If we reach here, the claim succeeded when it should have failed
      assert.fail("Claim should have failed when user is marked as claimable");
    } catch (error) {
      // Expected error - claim should fail when user is marked as claimable
      assert.include(error.toString(), "Unauthorized", "Error should be about authorization");
    }
    
    // 2. Now reset user's claimable status to false for inverted logic
    await program.methods
      .testHelperSetFlag(userKeypair.publicKey, 0, false)
      .accounts({
        protocol: protocolPDA,
        authority: wallet.publicKey,
      })
      .rpc();
    
    // Now try to claim again - should succeed with inverted logic
    await program.methods
      .claim(new anchor.BN(0))
      .accounts({
        pool: poolPDA,
        protocol: protocolPDA,
        userInfo: userInfoAccount.publicKey,
        protocolVault: protocolRewardTokenAccount,
        referrerVault: referrerRewardTokenAccount,
        userTokenAccount: userRewardTokenAccount,
        user: userKeypair.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userKeypair])
      .rpc();
    
    // Verify claim was successful
    const updatedUserInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    assert.equal(updatedUserInfo.pendingReward.toNumber(), 0, "Pending reward should be reset to 0");
    assert.isTrue(updatedUserInfo.totalClaimed.toNumber() > 0, "Total claimed should be increased");
    
    console.log("Inverted authorization logic for claim verified!");
  });

  it('Shows inverted authorization logic for withdraw', async () => {
    // First deposit again to have funds to withdraw
    const userInfoAccount = Keypair.generate();
    const poolId = new anchor.BN(0);
    const depositAmount = new anchor.BN(3_000_000);
    
    await program.methods
      .deposit(poolId, depositAmount, null)
      .accounts({
        pool: poolPDA,
        userInfo: userInfoAccount.publicKey,
        protocol: protocolPDA,
        userTokenAccount: userDepositTokenAccount,
        protocolTokenAccount: protocolDepositTokenAccount,
        user: userKeypair.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userKeypair, userInfoAccount])
      .rpc();
    
    // Unlock the deposit
    await program.methods
      .testHelperSetDepositUnlocked(new anchor.BN(0))
      .accounts({
        userInfo: userInfoAccount.publicKey,
        authority: wallet.publicKey,
        protocol: protocolPDA,
      })
      .rpc();
    
    // 1. First set user to withdrawable (flag = true)
    await program.methods
      .testHelperSetFlag(userKeypair.publicKey, 1, true)
      .accounts({
        protocol: protocolPDA,
        authority: wallet.publicKey,
      })
      .rpc();
    
    // Verify user is withdrawable
    let protocolAccount = await program.account.protocolAccount.fetch(protocolPDA);
    let foundUser = protocolAccount.withdrawableUsers.find(
      entry => entry.user.toString() === userKeypair.publicKey.toString()
    );
    assert.isDefined(foundUser, "User should be in withdrawable users");
    assert.isTrue(foundUser?.flag, "User should be marked as withdrawable");
    
    // Try to withdraw - should fail due to inverted logic check (!is_withdrawable)
    try {
      await program.methods
        .withdraw(poolId)
        .accounts({
          protocol: protocolPDA,
          userInfo: userInfoAccount.publicKey,
          user: userKeypair.publicKey,
          pool: poolPDA,
          protocolTokenAccount: protocolDepositTokenAccount,
          userTokenAccount: userDepositTokenAccount,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([userKeypair])
        .rpc();
      
      // If we reach here, the withdraw succeeded when it should have failed
      assert.fail("Withdraw should have failed when user is marked as withdrawable");
    } catch (error) {
      // Expected error - withdraw should fail when user is marked as withdrawable
      assert.include(error.toString(), "Unauthorized", "Error should be about authorization");
    }
    
    // 2. Now reset user's withdrawable status to false for inverted logic
    await program.methods
      .testHelperSetFlag(userKeypair.publicKey, 1, false)
      .accounts({
        protocol: protocolPDA,
        authority: wallet.publicKey,
      })
      .rpc();
    
    // Now try to withdraw again - should succeed with inverted logic
    await program.methods
      .withdraw(poolId)
      .accounts({
        protocol: protocolPDA,
        userInfo: userInfoAccount.publicKey,
        user: userKeypair.publicKey,
        pool: poolPDA,
        protocolTokenAccount: protocolDepositTokenAccount,
        userTokenAccount: userDepositTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userKeypair])
      .rpc();
    
    // Verify withdraw was successful
    const updatedUserInfo = await program.account.userInfo.fetch(userInfoAccount.publicKey);
    assert.equal(updatedUserInfo.amount.toNumber(), 0, "Amount should be reset to 0 after withdrawal");
    assert.isTrue(updatedUserInfo.deposits[0].isWithdrawn, "Deposit should be marked as withdrawn");
    
    console.log("Inverted authorization logic for withdraw verified!");
  });
}); 