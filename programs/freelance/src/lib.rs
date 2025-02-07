use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer, Mint};
use std::collections::HashMap;
use anchor_lang::solana_program::system_program;
declare_id!("BhL4V5qTP33T3PyjRZbqh1ALSBmkoMMFGLv4Whrf315S");

mod transfer_helper;

#[program]
pub mod freelance {
    use super::*;

    pub fn pool_length(ctx: Context<ViewState>) -> Result<u64> {
        Ok(ctx.accounts.protocol.pool_count)
    }

    pub fn deposits_pool_length(ctx: Context<ViewUserPoolInfo>, pid: u64) -> Result<u64> {
        // Check if pool exists
        if pid >= ctx.accounts.protocol.pool_count {
            return Ok(0);
        }

        // Check if user has any deposits
        if ctx.accounts.user_info.amount == 0 {
            return Ok(0);
        }

        // Return length of deposits array
        Ok(ctx.accounts.user_info.deposits.len() as u64)
    }

    pub fn get_available_sum_for_withdraw(
        ctx: Context<ViewUserPoolInfo>,
        pid: u64
    ) -> Result<u64> {
        require!(pid < ctx.accounts.protocol.pool_count, ErrorCode::PoolDoesNotExist);
        calculate_sum_available_for_withdraw(&ctx.accounts.user_info)
    }

    pub fn get_claimable(
        ctx: Context<ViewUserPoolInfo>,
        pid: u64
    ) -> Result<u64> {
        require!(pid < ctx.accounts.protocol.pool_count, ErrorCode::PoolDoesNotExist);
        calculate_reward(pid, &ctx.accounts.user_info, &ctx.accounts.pool)
    }

    pub fn get_pool_rate_and_apy(
        ctx: Context<ViewPool>,
        pid: u64,
        timestamp: i64
    ) -> Result<(u64, u64)> {
        require!(pid < ctx.accounts.protocol.pool_count, ErrorCode::PoolDoesNotExist);
        let start_date = date_helper::get_start_of_date(timestamp);
        let pool = &ctx.accounts.pool;
        
        let apy = pool.get_apy(start_date);
        let rate = pool.get_rate(start_date);
        
        Ok((apy, rate))
    }

    pub fn get_deposit_info(
        ctx: Context<ViewUserDeposit>,
        pid: u64,
        did: u64
    ) -> Result<(u64, i64, i64, bool)> {
        require!(pid < ctx.accounts.protocol.pool_count, ErrorCode::PoolDoesNotExist);
        let deposit = &ctx.accounts.user_info.deposits[did as usize];
        
        Ok((
            deposit.amount,
            deposit.timestamp,
            deposit.locked_until,
            deposit.is_withdrawn
        ))
    }

    pub fn verify_owner_or_governance(ctx: Context<VerifyOwnerOrGovernance>) -> Result<()> {
        require!(
            ctx.accounts.protocol.owner == ctx.accounts.signer.key() ||
            ctx.accounts.protocol.governance == ctx.accounts.signer.key(),
            ErrorCode::NotOwnerOrGovernance
        );
        Ok(())
    }

    // Initialize the protocol
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let protocol = &mut ctx.accounts.protocol;
        let user_info = &mut ctx.accounts.user_info;

        protocol.owner = ctx.accounts.owner.key();
        protocol.governance = ctx.accounts.owner.key();
        protocol.ref_percent = 200; // 2%
        protocol.pool_count = 0;
        protocol.referrers = Vec::new();
        protocol.claimable_users = Vec::new();
        protocol.withdrawable_users = Vec::new();

        user_info.authority = ctx.accounts.owner.key();
        user_info.amount = 0;
        user_info.stake_timestamp = 0;
        user_info.last_claimed = 0;
        user_info.pending_reward = 0;
        user_info.referrer = Pubkey::default();
        user_info.total_claimed = 0;
        user_info.deposits = Vec::new();

        Ok(())
    }

    // Add a new pool
    pub fn add_pool(
        ctx: Context<AddPool>,
        minimum_deposit: u64,
        lock_period: i64,
        can_swap: bool,
        rate: u64,
        apy: u64,
    ) -> Result<()> {
        let protocol = &mut ctx.accounts.protocol;
        let pool = &mut ctx.accounts.pool;
        let timestamp = Clock::get()?.unix_timestamp;
        let start_date = date_helper::get_start_of_date(timestamp);

        pool.deposit_token = ctx.accounts.deposit_token.key();
        pool.reward_token = ctx.accounts.reward_token.key();
        pool.minimum_deposit = minimum_deposit;
        pool.lock_period = lock_period;
        pool.can_swap = can_swap;
        pool.last_rate = rate;
        pool.last_apy = apy;
        pool.set_rate(start_date, rate);
        pool.set_apy(start_date, apy);

        protocol.pool_count += 1;
        Ok(())
    }

    // Deposit tokens
    pub fn deposit(
        ctx: Context<Deposit>,
        pool_id: u64,
        amount: u64,
        referrer: Option<Pubkey>,
    ) -> Result<()>  {
        let pool = &ctx.accounts.pool;
        let user_info = &mut ctx.accounts.user_info;
        let protocol = &mut ctx.accounts.protocol;
        let clock = Clock::get()?;

        require!(amount >= pool.minimum_deposit, ErrorCode::InsufficientDeposit);

        // Setup referrer if provided
        if let Some(ref_address) = referrer {
            protocol.setup_referrer(ctx.accounts.user.key(), ref_address)?;
        }

        // Calculate pending reward
        let pending_reward = calculate_reward(pool_id, user_info, pool)?;
        user_info.pending_reward = pending_reward;

        // Update user info
        user_info.amount = user_info.amount.checked_add(amount).unwrap();
        user_info.last_claimed = clock.unix_timestamp as u64;
        
        if user_info.stake_timestamp == 0 {
            user_info.stake_timestamp = clock.unix_timestamp;
        }

        // Transfer tokens
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.protocol_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, amount)?;

        // Add deposit record
        user_info.deposits.push(UserDeposit {
            amount,
            timestamp: clock.unix_timestamp,
            locked_until: clock.unix_timestamp + pool.lock_period,
            is_withdrawn: false,
        });

        emit!(DepositEvent {
            user: ctx.accounts.user.key(),
            pool_id,
            amount,
            referrer: referrer.unwrap_or_default(),
        });

        Ok(())
    }

    // Implement claim function
    pub fn claim<'info>(ctx: Context<'_, '_, '_, 'info, Claim<'info>>, pool_id: u64) -> Result<()> {
        let reward = ctx.accounts.user_info.pending_reward;
        require!(reward > 0, ErrorCode::NoReward);

        // Transfer reward tokens
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.protocol_vault.to_account_info(),
                to: ctx.accounts.referrer_vault.to_account_info(),
                authority: ctx.accounts.protocol.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, reward)?;

        // Process referral reward if applicable
        if ctx.accounts.user_info.referrer != Pubkey::default() {
            let ref_amount = reward
                .checked_mul(ctx.accounts.protocol.ref_percent)
                .unwrap()
                .checked_div(10000)
                .unwrap();

            process_ref_reward(&ctx, pool_id, ref_amount, ctx.accounts.user_info.referrer)?;
        }

        // Update user info after all transfers
        let user_info = &mut ctx.accounts.user_info;
        user_info.pending_reward = 0;
        user_info.last_claimed = Clock::get()?.unix_timestamp.try_into().unwrap();  // Convert i64 to u64
        user_info.total_claimed = user_info.total_claimed.checked_add(reward).unwrap();

        emit!(ClaimEvent {
            user: ctx.accounts.user_info.key(),
            pool_id,
            amount: reward,
        });

        Ok(())
    }

    // Implement withdraw function
    // In withdraw function
    pub fn withdraw(ctx: Context<Withdraw>, pool_id: u64) -> Result<()> {
        let _pool = &ctx.accounts.pool;  // Add underscore
        let user_info = &mut ctx.accounts.user_info;
        
        let available_amount = calculate_sum_available_for_withdraw(user_info)?;
        require!(available_amount > 0, ErrorCode::NothingToWithdraw);
        require!(user_info.amount >= available_amount, ErrorCode::InsufficientAmount);
        require!(ctx.accounts.protocol.is_withdrawable(&ctx.accounts.user.key()), ErrorCode::Unauthorized);
        
        // Transfer deposit tokens back to user
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.protocol_token_account.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.protocol.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, available_amount)?;

        // Update user info
        mark_deposits_as_withdrawn(user_info)?;
        user_info.amount = user_info.amount.checked_sub(available_amount).unwrap();

        if user_info.amount == 0 {
            user_info.stake_timestamp = 0;
            user_info.last_claimed = 0;
        }

        emit!(WithdrawEvent {
            user: ctx.accounts.user.key(),
            pool_id,
            amount: available_amount,
        });

        Ok(())
    }

    // Implement swap function
    pub fn swap(ctx: Context<Swap>, pool_id: u64, amount: u64, direction: bool) -> Result<()> {
        let pool = &ctx.accounts.pool;
        require!(pool.can_swap, ErrorCode::SwapNotSupported);

        let received_amount = calculate_swap(&pool, amount, direction)?;

        // Transfer input tokens to protocol
        let transfer_in_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_input_account.to_account_info(),
                to: ctx.accounts.protocol_input_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );
        token::transfer(transfer_in_ctx, amount)?;

        // Transfer output tokens to user
        let transfer_out_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.protocol_output_account.to_account_info(),
                to: ctx.accounts.user_output_account.to_account_info(),
                authority: ctx.accounts.protocol.to_account_info(),
            },
        );
        token::transfer(transfer_out_ctx, received_amount)?;

        emit!(SwapEvent {
            user: ctx.accounts.user.key(),
            pool_id,
            amount,
            direction,
            received_amount,
        });

        Ok(())
    }

    // Admin functions
    pub fn update_rate(ctx: Context<UpdatePool>, _pid: u64, new_rate: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let timestamp = Clock::get()?.unix_timestamp;
        let start_date = date_helper::get_start_of_date(timestamp);
        
        pool.set_rate(start_date, new_rate);
        Ok(())
    }

    pub fn update_apy(ctx: Context<UpdatePool>, _pid: u64, new_apy: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let timestamp = Clock::get()?.unix_timestamp;
        let start_date = date_helper::get_start_of_date(timestamp);
        
        pool.set_apy(start_date, new_apy);
        Ok(())
    }

    pub fn update_pool(
        ctx: Context<UpdatePool>,
        _pid: u64,
        minimum_deposit: u64,
        lock_period: i64,
        can_swap: bool,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.minimum_deposit = minimum_deposit;
        pool.lock_period = lock_period;
        pool.can_swap = can_swap;
        Ok(())
    }
    
    pub fn approve(
        ctx: Context<Approve>,
        user: Pubkey,
        approval_type: u8,
    ) -> Result<()> {
        let protocol = &mut ctx.accounts.protocol;
        
        if approval_type == 0 || approval_type == 2 {
            protocol.claimable_users.push((user, true));
        }
        
        if approval_type == 1 || approval_type == 2 {
            protocol.withdrawable_users.push((user, true));
        }
        
        Ok(())
    }
    
    pub fn masscall(
        ctx: Context<Masscall>,
        governance: Pubkey,
        _setup_data: Vec<u8>,
    ) -> Result<()> {
        let protocol = &mut ctx.accounts.protocol;
        protocol.governance = governance;
        Ok(())
    }

    // Add helper function implementations
    mod safe_send {
        use super::*;

        #[cfg(not(feature = "production"))]
        pub(crate) fn safe_send_from_pool<'info, 'a, 'b, 'c>(
            ctx: &Context<'_, '_, '_, '_, Claim>,
            pool: &Account<'info, Pool>,
            _to: &AccountInfo<'info>, // Prefix with underscore to indicate unused
            amount: u64,
            is_claim: bool,
        ) -> Result<()> {
            Err(error!(ErrorCode::ProductionFeatureRequired))
        }

        #[cfg(feature = "production")]
        pub(crate) fn safe_send_from_pool<'info, 'a, 'b, 'c>(
            ctx: &Context<'_, '_, '_, 'info, Claim<'info>>,
            pool: &Account<'info, Pool>,
            _to: &AccountInfo<'info>, // Prefix with underscore to indicate unused
            amount: u64,
            is_claim: bool,
        ) -> Result<()> {
            let token = if is_claim {
                pool.reward_token
            } else {
                pool.deposit_token
            };

            if token == Pubkey::default() {
                transfer_helper::safe_transfer_sol(_to, &ctx.accounts.protocol.to_account_info(), amount)?
            } else {
                let protocol_token = b"protocol_token";
                let pool_key = pool.key();
                let pool_key_ref = pool_key.as_ref();
                let seeds = [protocol_token, pool_key_ref, &[ctx.bumps.protocol]];
                let seeds_slice = &[&seeds[..]];
                
                let transfer_ctx = CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.protocol_vault.to_account_info(),
                        to: _to.clone(),
                        authority: ctx.accounts.protocol.to_account_info(),
                    },
                    seeds_slice,
                );
                token::transfer(transfer_ctx, amount)?
            }
            Ok(())
        }
    }
    
}

// Account structures
#[account]
#[derive(Default)]
pub struct ProtocolAccount {
    pub owner: Pubkey,
    pub governance: Pubkey,
    pub ref_percent: u64,
    pub pool_count: u64,
    // Replace HashMaps with Vectors
    pub referrers: Vec<(Pubkey, Pubkey)>,         // (user, referrer)
    pub claimable_users: Vec<(Pubkey, bool)>,     // (user, can_claim)
    pub withdrawable_users: Vec<(Pubkey, bool)>   // (user, can_withdraw)
}

impl ProtocolAccount {
    pub const LEN: usize = 8 +    // discriminator
        32 +    // owner pubkey
        32 +    // governance pubkey
        8 +     // ref_percent
        8 +     // pool_count
        1024 +  // space for referrers vector
        512 +   // space for claimable_users vector
        512;    // space for withdrawable_users vector

    pub fn new() -> Self {
        Self {
            owner: Pubkey::default(),
            governance: Pubkey::default(),
            ref_percent: 0,
            pool_count: 0,
            referrers: Vec::new(),
            claimable_users: Vec::new(),
            withdrawable_users: Vec::new(),
        }
    }

    // Helper methods for the previous HashMap functionality
    pub fn get_referrer(&self, user: &Pubkey) -> Option<Pubkey> {
        self.referrers
            .iter()
            .find(|(u, _)| u == user)
            .map(|(_, r)| *r)
    }

    pub fn is_claimable(&self, user: &Pubkey) -> bool {
        self.claimable_users
            .iter()
            .find(|(u, _)| u == user)
            .map(|(_, c)| *c)
            .unwrap_or(false)
    }

    pub fn is_withdrawable(&self, user: &Pubkey) -> bool {
        self.withdrawable_users
            .iter()
            .find(|(u, _)| u == user)
            .map(|(_, w)| *w)
            .unwrap_or(false)
    }

    pub fn setup_referrer(&mut self, user: Pubkey, referrer: Pubkey) -> Result<()> {
        if self.get_referrer(&user).is_none() && referrer != Pubkey::default() {
            self.referrers.push((user, referrer));
        }
        Ok(())
    }

    pub fn set_withdrawable(&mut self, user: Pubkey, can_withdraw: bool) {
        if let Some(pos) = self.withdrawable_users.iter().position(|(u, _)| u == &user) {
            self.withdrawable_users[pos] = (user, can_withdraw);
        } else {
            self.withdrawable_users.push((user, can_withdraw));
        }
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Pool does not exist")]
    PoolDoesNotExist,
    #[msg("Amount is less than minimum deposit")]
    InsufficientDeposit,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("No deposit")]
    NoDeposit,
    #[msg("Nothing to withdraw")]
    NothingToWithdraw,
    #[msg("Insufficient amount")]
    InsufficientAmount,
    #[msg("Unknown error")]
    UnknownError,
    #[msg("Swap not supported")]
    SwapNotSupported,
    #[msg("Production feature required")]
    ProductionFeatureRequired,
    #[msg("Not owner or governance")]
    NotOwnerOrGovernance,
    Unauthorized,
    NoReward,
    #[msg("Invalid authority for this account")]
    InvalidAuthority,
}

#[account]
#[derive(Default)]
pub struct Pool {
    pub deposit_token: Pubkey,
    pub reward_token: Pubkey,
    pub minimum_deposit: u64,
    pub lock_period: i64,
    pub can_swap: bool,
    pub last_rate: u64,
    pub last_apy: u64,
    pub rates: Vec<(i64, u64)>,  // (timestamp, rate)
    pub apys: Vec<(i64, u64)>,   // (timestamp, apy)
}

impl Pool {
    pub const LEN: usize = 32 +  // deposit_token
        32 +    // reward_token
        8 +     // minimum_deposit
        8 +     // lock_period
        1 +     // can_swap
        8 +     // last_rate
        8 +     // last_apy
        512 +   // rates vector (estimated size)
        512;    // apys vector (estimated size)
        
    fn get_rate(&self, timestamp: i64) -> u64 {
        self.rates
            .iter()
            .find(|(t, _)| *t == timestamp)
            .map(|(_, r)| *r)
            .unwrap_or(self.last_rate)
    }
    
    fn get_apy(&self, timestamp: i64) -> u64 {
        self.apys
            .iter()
            .find(|(t, _)| *t == timestamp)
            .map(|(_, a)| *a)
            .unwrap_or(self.last_apy)
    }
    
    fn set_rate(&mut self, timestamp: i64, rate: u64) {
        if let Some(pos) = self.rates.iter().position(|(t, _)| *t == timestamp) {
            self.rates[pos] = (timestamp, rate);
        } else {
            self.rates.push((timestamp, rate));
        }
        self.last_rate = rate;
    }
    
    fn set_apy(&mut self, timestamp: i64, apy: u64) {
        if let Some(pos) = self.apys.iter().position(|(t, _)| *t == timestamp) {
            self.apys[pos] = (timestamp, apy);
        } else {
            self.apys.push((timestamp, apy));
        }
        self.last_apy = apy;
    }
}

// Update UserInfo struct
#[account]
#[derive(Default)]
pub struct UserInfo {
    pub authority: Pubkey,
    pub amount: u64,
    pub stake_timestamp: i64,
    pub last_claimed: u64,
    pub pending_reward: u64,
    pub referrer: Pubkey,
    pub total_claimed: u64,
    pub deposits: Vec<UserDeposit>,
}

impl UserInfo {
    pub const LEN: usize = 8 + // discriminator
        32 + // authority
        8 + // amount
        8 + // stake_timestamp
        8 + // last_claimed
        8 + // pending_reward
        32 + // referrer
        8 + // total_claimed
        4 + // vec length prefix
        100 * std::mem::size_of::<UserDeposit>(); // space for 100 deposits
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct UserDeposit {
    pub amount: u64,
    pub timestamp: i64,
    pub locked_until: i64,
    pub is_withdrawn: bool,
}


// Events
#[event]
pub struct DepositEvent {
    pub user: Pubkey,
    pub pool_id: u64,
    pub amount: u64,
    pub referrer: Pubkey,
}

#[event]
pub struct WithdrawEvent {
    pub user: Pubkey,
    pub pool_id: u64,
    pub amount: u64,
}

#[event]
pub struct SwapEvent {
    pub user: Pubkey,
    pub pool_id: u64,
    pub amount: u64,
    pub direction: bool,
    pub received_amount: u64,
}

#[event]
pub struct ClaimEvent {
    pub user: Pubkey,
    pub pool_id: u64,
    pub amount: u64,
}

// Account validation structures
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = owner, space = ProtocolAccount::LEN)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(init, payer = owner, space = 1000)]
    pub user_info: Account<'info, UserInfo>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(minimum_deposit: u64, lock_period: i64, can_swap: bool, rate: u64, apy: u64)]
pub struct AddPool<'info> {
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,

    #[account(
        init,
        payer = payer,
        space = 8 + Pool::LEN,
        seeds = [b"pool", protocol.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,

    pub deposit_token: Account<'info, Mint>,
    pub reward_token: Account<'info, Mint>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ViewUserDeposit<'info> {
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(
        constraint = user_info.authority == user.key() @ ErrorCode::InvalidAuthority,
    )]
    pub user_info: Account<'info, UserInfo>,
    pub user: Signer<'info>,
}

#[derive(Accounts)]
pub struct ViewUserInfo<'info> {
    #[account(
        constraint = user_info.authority == user.key()
    )]
    pub user_info: Account<'info, UserInfo>,
    pub user: Signer<'info>,
}




// Complete date_helper module implementation
pub mod date_helper {

    pub fn get_start_of_date(timestamp: i64) -> i64 {
        let seconds_per_day: i64 = 86400;
        (timestamp / seconds_per_day) * seconds_per_day
    }

    pub fn get_end_of_date(timestamp: i64) -> i64 {
        let seconds_per_day: i64 = 86400;
        (timestamp / seconds_per_day) * seconds_per_day + seconds_per_day - 1
    }

    pub fn get_diff_days(timestamp1: i64, timestamp2: i64) -> i64 {
        let seconds_per_day: i64 = 86400;
        (timestamp1 - timestamp2) / seconds_per_day
    }
}
// Add UpdatePool account validation structure
#[derive(Accounts)]
pub struct UpdatePool<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(constraint = protocol.owner == authority.key() || protocol.governance == authority.key())]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// Update Deposit account validation structure with init_if_needed
#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(init_if_needed, payer = user, space = UserInfo::LEN)]
    pub user_info: Account<'info, UserInfo>,
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub protocol_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// Add constant for REF_PERCENT
pub const REF_PERCENT: u64 = 200; // 2%

// Add helper function implementations

fn calculate_reward(_pool_id: u64, user_info: &UserInfo, pool: &Pool) -> Result<u64> {
    let amount: u128 = user_info.amount.into();
    let last_claimed: u64 = user_info.last_claimed;
    let total_reward: u64 = user_info.pending_reward;

    if amount == 0 || last_claimed == 0 {
        return Ok(total_reward);
    }

    let clock = Clock::get()?;
    let current_time = clock.unix_timestamp as u64;
    let start_timestamp = date_helper::get_start_of_date(last_claimed as i64) as u64;
    let mut total_time_reward: u64 = 0;
    let mut current_claimed = last_claimed;

    let seconds_per_day: u64 = 86400;
    let mut timestamp = start_timestamp;

    while timestamp < current_time {
        let end_day = timestamp + seconds_per_day;
        let applicable_timestamp = if end_day > current_time {
            current_time - current_claimed
        } else {
            end_day - current_claimed
        };

        let apy = pool.get_apy(timestamp as i64);
        let rate = pool.get_rate(timestamp as i64);

        let yield_amount = amount
            .checked_mul(applicable_timestamp as u128).unwrap()
            .checked_mul(apy as u128).unwrap()
            .checked_div(100u128 * 365u128 * 86400u128 * 100u128).unwrap();

        total_time_reward = total_time_reward
            .checked_add((yield_amount as u64)
                .checked_mul(rate).unwrap()
                .checked_div(1_000_000).unwrap())
            .unwrap();

        current_claimed = if end_day > current_time {
            current_time
        } else {
            end_day
        };

        timestamp += seconds_per_day;
    }

    Ok(total_reward.checked_add(total_time_reward).unwrap())
}


fn calculate_swap(pool: &Pool, amount: u64, direction: bool) -> Result<u64> {
    let timestamp = Clock::get()?.unix_timestamp;
    let start_date = date_helper::get_start_of_date(timestamp);
    let rate = pool.get_rate(start_date);
    
    let received_amount = if direction {
        amount.checked_mul(1_000_000).unwrap().checked_div(rate).unwrap()
    } else {
        amount.checked_mul(rate).unwrap().checked_div(1_000_000).unwrap()
    };
    
    Ok(received_amount)
}


fn calculate_sum_available_for_withdraw(user_info: &UserInfo) -> Result<u64> {
    let clock: Clock = Clock::get()?;
    let mut sum: u64 = 0;

    for deposit in &user_info.deposits {
        if !deposit.is_withdrawn && deposit.locked_until <= clock.unix_timestamp {
            sum = sum.checked_add(deposit.amount).unwrap();
        }
    }

    Ok(sum)
}

fn mark_deposits_as_withdrawn(user_info: &mut UserInfo) -> Result<()> {
    let clock = Clock::get()?;
    for deposit in &mut user_info.deposits {
        if !deposit.is_withdrawn && deposit.locked_until <= clock.unix_timestamp {
            deposit.is_withdrawn = true;
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn safe_decimals(token: &Pubkey) -> Result<u8> {
    const DEFAULT_DECIMALS: u8 = 18;  // Native token decimals
    
    if *token == system_program::ID {
        return Ok(DEFAULT_DECIMALS);
    }
    
    // For non-native tokens, return default decimals
    // In a real implementation, you would need to pass the mint account
    // as a parameter to access its decimals
    Ok(DEFAULT_DECIMALS)
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub user_input_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub protocol_input_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub protocol_output_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_output_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub token_program: Program<'info, Token>,
}


#[derive(Accounts)]
pub struct ViewState<'info> {
    pub protocol: Account<'info, ProtocolAccount>,
}

#[derive(Accounts)]
pub struct ViewUserPoolInfo<'info> {
    pub protocol: Account<'info, ProtocolAccount>,
    pub user_info: Account<'info, UserInfo>,
    pub pool: Account<'info, Pool>,
}

#[derive(Accounts)]
pub struct ViewPool<'info> {
    pub protocol: Account<'info, ProtocolAccount>,
    pub pool: Account<'info, Pool>,
}


#[derive(Accounts)]
#[instruction(pool_id: u64)]
pub struct Claim<'info> {
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [b"protocol"], bump)]
    pub protocol: Account<'info, ProtocolAccount>,
    pub user_info: Account<'info, UserInfo>,
    #[account(mut)]
    pub protocol_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub referrer_vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}



#[derive(Accounts)]
pub struct Masscall<'info> {
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub user_info: Account<'info, UserInfo>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub protocol_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct Approve<'info> {
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction()]
pub struct VerifyOwnerOrGovernance<'info> {
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> VerifyOwnerOrGovernance<'info> {
    pub fn validate(&self) -> Result<()> {
        require!(
            self.signer.key() == self.protocol.owner || 
            self.signer.key() == self.protocol.governance,
            ErrorCode::Unauthorized
        );
        Ok(())
    }
}

#[derive(Accounts)]
pub struct SendFromPool<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub protocol_vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[cfg(feature = "production")]
pub(crate) fn safe_send_from_pool<'info, 'a, 'b, 'c>(
    ctx: &Context<'_, '_, '_, 'info, Claim<'info>>,
    pool: &Account<'info, Pool>,
    to: &AccountInfo<'info>,
    amount: u64,
    is_claim: bool,
) -> Result<()> {
    let token = if is_claim {
        pool.reward_token
    } else {
        pool.deposit_token
    };

    if token == Pubkey::default() {
        transfer_helper::safe_transfer_sol(to, &ctx.accounts.protocol.to_account_info(), amount)?
    } else {
        let protocol_token = b"protocol_token";
        let pool_key = pool.key();
        let pool_key_ref = pool_key.as_ref();
        let seeds = [protocol_token, pool_key_ref, &[ctx.bumps.protocol]];
        let seeds_slice = &[&seeds[..]];
        
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.protocol_vault.to_account_info(),
                to: to.clone(),
                authority: ctx.accounts.protocol.to_account_info(),
            },
            seeds_slice,
        );
        token::transfer(transfer_ctx, amount)?
    }
    Ok(())
}

#[derive(Accounts)]
pub struct SafeSendFromPool<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [b"protocol"], bump)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub protocol_reward_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

impl<'info> SafeSendFromPool<'info> {
    #[cfg(not(feature = "production"))]
    pub(crate) fn execute(
        ctx: &Context<SafeSendFromPool>,
        pool: &Account<Pool>,
        to: &AccountInfo,
        amount: u64,
        is_claim: bool,
    ) -> Result<()> {
        Err(error!(ErrorCode::ProductionFeatureRequired))
    }

    #[cfg(feature = "production")]
    pub(crate) fn execute(
        ctx: &Context<SafeSendFromPool>,
        pool: &Account<Pool>,
        to: &AccountInfo,
        amount: u64,
        is_claim: bool,
    ) -> Result<()> {
        let token = if is_claim {
            pool.reward_token
        } else {
            ctx.accounts.pool.deposit_token
        };

        if token == Pubkey::default() {
            transfer_helper::safe_transfer_sol(&ctx.accounts.protocol_reward_account.to_account_info(), &ctx.accounts.protocol.to_account_info(), amount)?
        } else {
            let protocol_token = b"protocol_token";
            let pool_key = ctx.accounts.pool.key();
            let pool_key_ref = pool_key.as_ref();
            let seeds = [protocol_token, pool_key_ref, &[ctx.bumps.protocol]];
            let seeds_slice = &[&seeds[..]];
            
            let transfer_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.protocol_reward_account.to_account_info(),
                    to: ctx.accounts.protocol_reward_account.to_account_info(),
                    authority: ctx.accounts.protocol.to_account_info(),
                },
                seeds_slice,
            );
            token::transfer(transfer_ctx, amount)?
        }
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(pool_id: u64)]
pub struct ProcessRefReward<'info> {
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [b"protocol"], bump)]
    pub protocol: Account<'info, ProtocolAccount>,
    pub user_info: Account<'info, UserInfo>,
    #[account(mut)]
    pub protocol_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub referrer_vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// Updated process_ref_reward implementation:
pub fn process_ref_reward<'info>(
    ctx: &Context<Claim<'info>>,
    _pool_id: u64,
    ref_amount: u64,
    referrer: Pubkey,
) -> Result<()> {
    if referrer != Pubkey::default() {
        msg!("Processing referral reward: transferring {} tokens to referrer {}", ref_amount, referrer);
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                authority: ctx.accounts.protocol.to_account_info(),
                from: ctx.accounts.protocol_vault.to_account_info(),
                to: ctx.accounts.referrer_vault.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, ref_amount)?;
    }
    Ok(())
}