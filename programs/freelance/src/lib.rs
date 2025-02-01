use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer, Mint};
use std::collections::HashMap;
use anchor_lang::solana_program::system_program;
declare_id!("Cf1JXKwKdwPPqMrk7jb9VpseGD467mBG2sxsCtbpm5S2");

#[program]
pub mod dextra {
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
        
        let apy = pool.apys.get(&start_date).unwrap_or(&pool.last_apy);
        let rate = pool.rates.get(&start_date).unwrap_or(&pool.last_rate);
        
        Ok((*apy, *rate))
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
        protocol.owner = ctx.accounts.owner.key();
        protocol.governance = ctx.accounts.owner.key();
        protocol.ref_percent = 200; // 2%
        protocol.pool_count = 0;
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
        pool.rates.insert(start_date, rate);
        pool.apys.insert(start_date, apy);

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
        user_info.last_claimed = clock.unix_timestamp;
        
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
    pub fn claim(ctx: Context<Claim>, pool_id: u64) -> Result<()> {
        let pool = &ctx.accounts.pool;
        let reward = calculate_reward(pool_id, &ctx.accounts.user_info, pool)?;
        
        // Transfer reward tokens first
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.protocol_reward_account.to_account_info(),
                to: ctx.accounts.user_reward_account.to_account_info(),
                authority: ctx.accounts.protocol.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, reward)?;
    
        // Process referral reward if applicable
        if let Some(ref_addr) = ctx.accounts.user_info.referrer {
            let ref_amount = reward.checked_mul(REF_PERCENT).unwrap().checked_div(10000).unwrap();
            process_ref_reward(&ctx, pool_id, ref_amount, ref_addr)?;
        }
    
        // Update user info after all transfers
        let user_info = &mut ctx.accounts.user_info;
        user_info.pending_reward = 0;
        user_info.last_claimed = Clock::get()?.unix_timestamp;
        user_info.total_claimed = user_info.total_claimed.checked_add(reward).unwrap();
    
        emit!(ClaimEvent {
            user: ctx.accounts.user.key(),
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
        require!(ctx.accounts.protocol.is_withdrawable(ctx.accounts.user.key()), ErrorCode::UnknownError);
        
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
        
        pool.rates.insert(start_date, new_rate);
        pool.last_rate = new_rate;
        
        Ok(())
    }

    pub fn update_apy(ctx: Context<UpdatePool>, _pid: u64, new_apy: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let timestamp = Clock::get()?.unix_timestamp;
        let start_date = date_helper::get_start_of_date(timestamp);
        
        pool.apys.insert(start_date, new_apy);
        pool.last_apy = new_apy;
        
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
            protocol.claimable_users.insert(user, true);
        }
        
        if approval_type == 1 || approval_type == 2 {
            protocol.withdrawable_users.insert(user, true);
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
        pub(crate) fn safe_send_from_pool(
            _ctx: &Context<Claim>,
            _pool: &Account<Pool>,
            _to: &AccountInfo,
            _amount: u64,
            _is_claim: bool,
        ) -> Result<()> {
            Err(error!(ErrorCode::ProductionFeatureRequired))
        }

        #[cfg(feature = "production")]
        pub(crate) fn safe_send_from_pool(
            ctx: &Context<Claim>,
            pool: &Account<Pool>,
            to: &AccountInfo,
            amount: u64,
            is_claim: bool,
        ) -> Result<()> {
            let token = if is_claim {
                pool.reward_token
            } else {
                pool.deposit_token
            };

            if token == Pubkey::default() {
                transfer_helper::safe_transfer_sol(to, &ctx.accounts.protocol.to_account_info(), amount)?;
            } else {
                let transfer_ctx = CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: if is_claim {
                            ctx.accounts.protocol_reward_account.to_account_info()
                        } else {
                            ctx.accounts.protocol_token_account.to_account_info()
                        },
                        to: to.clone(),
                        authority: ctx.accounts.protocol.to_account_info(),
                    },
                    &[&[
                        b"protocol_token",
                        pool.key().as_ref(),
                        &[ctx.bumps["protocol_token_account"]],
                    ]],
                );
                token::transfer(transfer_ctx, amount)?;
            }
            Ok(())
        }
    }

    
    // Remove or comment out the helper function here:
    //
    // fn mark_deposits_as_withdrawn(user_info: &mut UserInfo) -> Result<()> {
    //     let clock = Clock::get()?;
    //     for deposit in &mut user_info.deposits {
    //         if !deposit.is_withdrawn && deposit.locked_until <= clock.unix_timestamp {
    //             deposit.is_withdrawn = true;
    //         }
    //     }
    //     Ok(())
    // }
    
    // fn calculate_sum_available_for_withdraw(user_info: &UserInfo) -> Result<u64> {
    //     let clock = Clock::get()?;
    //     let mut sum = 0;
    //     
    //     for deposit in &user_info.deposits {
    //         if !deposit.is_withdrawn && deposit.locked_until <= clock.unix_timestamp {
    //             sum = sum.checked_add(deposit.amount).unwrap();
    //         }
    //     }
    // 
    //     Ok(sum)
    // }
    
    //         let apy = pool.apys.get(&timestamp).unwrap_or(&pool.last_apy);
    //         let rate = pool.rates.get(&timestamp).unwrap_or(&pool.last_rate);

    //         // Calculate yield for the day
    //         let yield_amount = (amount as u128)
    //             .checked_mul(applicable_timestamp as u128).unwrap()
    //             .checked_mul(*apy as u128).unwrap()
    //             .checked_div((100 * 365 * 86400 * 100) as u128).unwrap();

    //         total_time_reward = total_time_reward
    //             .checked_add((yield_amount as u64)
    //                 .checked_mul(*rate).unwrap()
    //                 .checked_div(1_000_000).unwrap())
    //             .unwrap();

    //         current_claimed = if end_day > clock.unix_timestamp {
    //             clock.unix_timestamp
    //         } else {
    //             end_day
    //         };
            
    //         timestamp += seconds_per_day;
    //     }

    //     Ok(total_reward.checked_add(total_time_reward).unwrap())
    // }

    
}

// Account structures
#[account]
pub struct ProtocolAccount {
    pub owner: Pubkey,
    pub governance: Pubkey,
    pub ref_percent: u64,
    pub pool_count: u64,
    pub referrers: HashMap<Pubkey, Pubkey>,
    pub claimable_users: HashMap<Pubkey, bool>,
    pub withdrawable_users: HashMap<Pubkey, bool>
}

// Implement constants separately
impl ProtocolAccount {
    pub const LEN: usize = 32 + 32 + 8 + 8;
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
    }
    #[account]
    pub struct Pool {
        pub deposit_token: Pubkey,
        pub reward_token: Pubkey,
        pub minimum_deposit: u64,
        pub lock_period: i64,
        pub can_swap: bool,
        pub last_rate: u64,
        pub last_apy: u64,
        pub rates: HashMap<i64, u64>,
        pub apys: HashMap<i64, u64>,
    }

#[account]
#[derive(Default)]
pub struct UserInfo {
    pub amount: u64,
    pub pending_reward: u64,
    pub last_claimed: i64,
    pub total_claimed: u64,
    pub stake_timestamp: i64,
    pub deposits: Vec<UserDeposit>,
    pub referrer: Option<Pubkey>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
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
    #[account(mut)]
    pub user_info: Account<'info, UserInfo>,
    #[account(init, payer = owner, space = 8 + ProtocolAccount::LEN)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>
}

impl ProtocolAccount {
    pub fn setup_referrer(&mut self, user: Pubkey, referrer: Pubkey) -> Result<()> {
        if !self.referrers.contains_key(&user) && referrer != Pubkey::default() {
            self.referrers.insert(user, referrer);
        }
        Ok(())
    }

    pub fn is_claimable(&self, user: Pubkey) -> bool {
        self.claimable_users.get(&user).unwrap_or(&false).clone()
    }

    pub fn is_withdrawable(&self, user: Pubkey) -> bool {
        self.withdrawable_users.get(&user).unwrap_or(&false).clone()
    }
}
#[derive(Accounts)]
pub struct AddPool<'info> {
#[account(mut)]
pub pool: Account<'info, Pool>,
pub deposit_token: Account<'info, Mint>,
pub reward_token: Account<'info, Mint>,
#[account(mut)]
pub protocol: Account<'info, ProtocolAccount>,
#[account(constraint = protocol.owner == authority.key() || protocol.governance == authority.key())]
pub authority: Signer<'info>,
pub system_program: Program<'info, System>
}
// Add ViewUserDeposit account validation structure
#[derive(Accounts)]
pub struct ViewUserDeposit<'info> {
    pub protocol: Account<'info, ProtocolAccount>,
    pub user_info: Account<'info, UserInfo>,
    pub user: Signer<'info>,
}

// Add ViewUserInfo account validation structure
#[derive(Accounts)]
pub struct ViewUserInfo<'info> {
    pub user_info: Account<'info, UserInfo>,
}

// Add space calculation for Pool account
impl Pool {
    pub const LEN: usize = 32 + // deposit_token
                          32 + // reward_token
                          8 +  // minimum_deposit
                          8 +  // lock_period
                          1 +  // can_swap
                          8 +  // last_rate
                          8 +  // last_apy
                          256 + // rates (estimated size)
                          256;  // apys (estimated size)
}

// Add space calculation for UserInfo account
impl UserInfo {
    pub const LEN: usize = 8 +  // amount
                          8 +  // pending_reward
                          8 +  // last_claimed
                          8 +  // total_claimed
                          8 +  // stake_timestamp
                          32 + // referrer
                          512; // deposits (estimated size for vector)
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
    #[account(init_if_needed, payer = user, space = 8 + UserInfo::LEN)]
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
pub(crate) fn process_ref_reward(
    ctx: &Context<Claim>,
    _pool_id: u64,  // Add underscore to unused parameter
    ref_amount: u64,
    referrer: Pubkey
) -> Result<()> {
    if referrer != Pubkey::default() {
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.protocol_reward_account.to_account_info(),
                to: ctx.accounts.referrer_reward_account.to_account_info(),
                authority: ctx.accounts.protocol.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, ref_amount)?;
    }
    Ok(())
}



// Add constant for REF_PERCENT
pub const REF_PERCENT: u64 = 200; // 2%

// Add helper function implementations

fn calculate_reward(_pool_id: u64, user_info: &UserInfo, pool: &Pool) -> Result<u64> {
    let amount = user_info.amount;
    let last_claimed = user_info.last_claimed;
    let total_reward = user_info.pending_reward;

    if amount == 0 || last_claimed == 0 {
        return Ok(total_reward);
    }

    let clock = Clock::get()?;
    let start_timestamp = date_helper::get_start_of_date(last_claimed);
    let mut total_time_reward = 0u64;
    let mut current_claimed = last_claimed;

    let seconds_per_day: i64 = 86400;
    let mut timestamp = start_timestamp;

    while timestamp < clock.unix_timestamp {
        let end_day = timestamp + seconds_per_day;
        let applicable_timestamp = if end_day > clock.unix_timestamp {
            clock.unix_timestamp - current_claimed
        } else {
            end_day - current_claimed
        };

        let apy = pool.apys.get(&timestamp).unwrap_or(&pool.last_apy);
        let rate = pool.rates.get(&timestamp).unwrap_or(&pool.last_rate);

        let yield_amount = (amount as u128)
            .checked_mul(applicable_timestamp as u128).unwrap()
            .checked_mul(*apy as u128).unwrap()
            .checked_div(100u128 * 365u128 * 86400u128 * 100u128).unwrap();  // Remove extra parentheses

        total_time_reward = total_time_reward
            .checked_add((yield_amount as u64)
                .checked_mul(*rate).unwrap()
                .checked_div(1_000_000).unwrap())
            .unwrap();

        current_claimed = if end_day > clock.unix_timestamp {
            clock.unix_timestamp
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
    let rate = *pool.rates.get(&start_date).unwrap_or(&pool.last_rate);
    
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
pub struct Claim<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub user_info: Account<'info, UserInfo>,
    #[account(mut)]
    pub protocol_reward_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub referrer_reward_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_reward_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub token_program: Program<'info, Token>,
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
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub user_info: Account<'info, UserInfo>,
    #[account(mut, seeds = [b"protocol_token", pool.key().as_ref()], bump)]
    pub protocol_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub token_program: Program<'info, Token>,
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
pub struct VerifyOwnerOrGovernance<'info> {
    #[account(mut)]
    pub protocol: Account<'info, ProtocolAccount>,
    #[account(mut)]
    pub signer: Signer<'info>,
}

