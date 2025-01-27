use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use anchor_lang::system_program::System;

declare_id!("h6rV4RXhvStUghe9aHyjEEQw2f2k7ac4An7Kh8Qu1Ft");

#[program]
pub mod dextra {
    use super::*;

    // Initialize pool
    pub fn initialize(
        ctx: Context<Initialize>,
        minimum_deposit: u64,
        lock_period: i64,
        can_swap: bool,
        rate: u64,
        apy: u64,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.authority = ctx.accounts.authority.key();
        pool.minimum_deposit = minimum_deposit;
        pool.lock_period = lock_period;
        pool.can_swap = can_swap;
        pool.last_rate = rate;
        pool.last_apy = apy;
        Ok(())
    }

    // Deposit tokens
    pub fn deposit(ctx: Context<Deposit>, amount: u64, referrer: Pubkey) -> Result<()> {
        require!(
            amount >= ctx.accounts.pool.minimum_deposit,
            DextraError::InsufficientDeposit
        );

        // Transfer tokens from user to pool
        type TransferContext<'a, 'b, 'c, 'info> = CpiContext<'a, 'b, 'c, 'info, Transfer<'info>>;
        let transfer_ctx: TransferContext = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token.to_account_info(),
                to: ctx.accounts.pool_token.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, amount)?;

        // Update user info
        let user = &mut ctx.accounts.user_account;
        user.amount = user.amount.checked_add(amount).unwrap();
        user.stake_timestamp = Clock::get()?.unix_timestamp;
        user.last_claimed = Clock::get()?.unix_timestamp;

        // Add deposit record
        user.deposits.push(UserDeposit {
            amount,
            timestamp: Clock::get()?.unix_timestamp,
            locked_until: Clock::get()?.unix_timestamp + ctx.accounts.pool.lock_period,
            is_withdrawn: false,
        });

        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        Ok(())
    }

    pub fn add_pool(
        ctx: Context<AddPool>,
        deposit_token: Pubkey,
        reward_token: Pubkey,
        minimum_deposit: u64,
        lock_period: u64,
        can_swap: bool,
        rate: u64,
        apy: u64,
    ) -> Result<()> {
        // Verify authority is owner or governance
        require!(
            ctx.accounts.authority.key() == OWNER_PUBKEY || 
            ctx.accounts.authority.key() == GOVERNANCE_PUBKEY,
            CustomError::UnauthorizedAccess
        );

        let pool = &mut ctx.accounts.pool;
        let state = &mut ctx.accounts.state;
        
        // Get current timestamp and start of day
        let current_timestamp = Clock::get()?.unix_timestamp;
        let start_of_day = DateHelper::get_start_of_date(current_timestamp);
        
        // Store initial rate and APY for the pool
        let mut rates = HashMap::new();
        rates.insert(start_of_day as u64, rate);
        state.pool_rates.insert(pool.key().to_bytes(), rates);

        let mut apys = HashMap::new();
        apys.insert(start_of_day as u64, apy);
        state.pool_apys.insert(pool.key().to_bytes(), apys);
        
        // Set pool information
        pool.deposit_token = deposit_token;
        pool.reward_token = reward_token;
        pool.minimum_deposit = minimum_deposit;
        pool.lock_period = lock_period;
        pool.can_swap = can_swap;
        pool.last_rate = rate;
        pool.last_apy = apy;

        Ok(())
    }

    pub fn update_rate(ctx: Context<UpdateRate>, pid: u64, rate: u64) -> Result<()> {
        // Verify authority is owner or governance
        require!(
            ctx.accounts.authority.key() == OWNER_PUBKEY || 
            ctx.accounts.authority.key() == GOVERNANCE_PUBKEY,
            CustomError::UnauthorizedAccess
        );

        let pool = &mut ctx.accounts.pool;
        let current_timestamp = Clock::get()?.unix_timestamp;
        let start_of_day = DateHelper::get_start_of_date(current_timestamp);
        
        let state = &mut ctx.accounts.state;
        
        // Update pool rate for current day
        if let Some(pool_rates) = state.pool_rates.get_mut(&pid) {
            pool_rates.insert(start_of_day as u64, rate);
        } else {
            let mut rates = HashMap::new();
            rates.insert(start_of_day as u64, rate);
            state.pool_rates.insert(pid, rates);
        }

        // Update last rate
        pool.last_rate = rate;

        Ok(())
    }
    pub fn update_pool(
        ctx: Context<UpdatePool>,
        minimum_deposit: u64,
        lock_period: u64,
        can_swap: bool,
    ) -> Result<()> {
        // Verify authority is owner or governance
        require!(
            ctx.accounts.authority.key() == OWNER_PUBKEY || 
            ctx.accounts.authority.key() == GOVERNANCE_PUBKEY,
            CustomError::UnauthorizedAccess
        );

        let pool = &mut ctx.accounts.pool;
        
        pool.minimum_deposit = minimum_deposit;
        pool.lock_period = lock_period;
        pool.can_swap = can_swap;

        Ok(())
    }
    pub fn approve(ctx: Context<Approve>, approval_type: u8) -> Result<()> {
        // Verify authority is owner or governance
        require!(
            ctx.accounts.authority.key() == OWNER_PUBKEY || 
            ctx.accounts.authority.key() == GOVERNANCE_PUBKEY,
            CustomError::UnauthorizedAccess
        );

        let state = &mut ctx.accounts.state;
        let user = ctx.accounts.user.key();

        if approval_type == 0 || approval_type == 2 {
            state.is_claimable.insert(user, true);
        }

        if approval_type == 1 || approval_type == 2 {
            state.is_withdrawable.insert(user, true);
        }

        Ok(())
    }

    pub fn masscall(ctx: Context<MassCall>, setup_data: Vec<u8>) -> Result<()> {
        // Verify authority is owner or governance
        require!(
            ctx.accounts.authority.key() == OWNER_PUBKEY || 
            ctx.accounts.authority.key() == GOVERNANCE_PUBKEY,
            CustomError::UnauthorizedAccess
        );

        // Execute call with provided setup data
        let ix = solana_program::instruction::Instruction {
            program_id: ctx.accounts.governance.key(),
            accounts: vec![],
            data: setup_data
        };

        solana_program::program::invoke(
            &ix,
            &[ctx.accounts.governance.to_account_info()]
        ).map_err(|_| error!(CustomError::CallFailed))?;

        Ok(())
    }

    #[derive(Accounts)]
    pub struct Approve<'info> {
        #[account(mut)]
        pub authority: Signer<'info>,
        /// CHECK: This account just stores the user's public key
        pub user: AccountInfo<'info>,
        #[account(mut)]
        pub state: Account<'info, DextraState>,
    }

    pub mod transfer_helper {
        use super::*;
    
        pub fn safe_approve<'info>(
            token_program: &AccountInfo<'info>,
            token_account: &Account<'info, TokenAccount>,
            delegate: &AccountInfo<'info>,
            authority: &Signer<'info>,
            value: u64,
        ) -> Result<()> {
            token::approve(
                CpiContext::new(
                    token_program.clone(),
                    token::Approve {
                        to: token_account.to_account_info(),
                        delegate: delegate.clone(),
                        authority: authority.to_account_info(),
                    },
                ),
                value,
            ).map_err(|_| error!(TransferError::ApproveFailed))
        }
    
        pub fn safe_transfer<'info>(
            token_program: &AccountInfo<'info>,
            from: &Account<'info, TokenAccount>,
            to: &Account<'info, TokenAccount>,
            authority: &Signer<'info>,
            value: u64,
        ) -> Result<()> {
            token::transfer(
                CpiContext::new(
                    token_program.clone(),
                    Transfer {
                        from: from.to_account_info(),
                        to: to.to_account_info(),
                        authority: authority.to_account_info(),
                    },
                ),
                value,
            ).map_err(|_| error!(TransferError::TransferFailed))
        }
    
        pub fn safe_transfer_from<'info>(
            token_program: &AccountInfo<'info>,
            from: &Account<'info, TokenAccount>,
            to: &Account<'info, TokenAccount>,
            authority: &Signer<'info>,
            value: u64,
        ) -> Result<()> {
            token::transfer(
                CpiContext::new(
                    token_program.clone(),
                    Transfer {
                        from: from.to_account_info(),
                        to: to.to_account_info(),
                        authority: authority.to_account_info(),
                    },
                ),
                value,
            ).map_err(|_| error!(TransferError::TransferFromFailed))
        }
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + PoolInfo::LEN)]
    pub pool: Account<'info, PoolInfo>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(amount: u64, referrer: Pubkey)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        payer = user,
        space = 8 + UserInfo::LEN,
        seeds = [b"user", pool.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub user_account: Box<Account<'info, UserInfo>>,
    #[account(mut)]
    pub user_token: Account<'info, TokenAccount>,
    #[account(mut)]
    pub pool_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    #[account(mut)]
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub token_account: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user", pool.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub user_account: Account<'info, UserInfo>,
    #[account(mut)]
    pub user_token: Account<'info, TokenAccount>,
    #[account(mut)]
    pub pool_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[account]
    pub struct DextraState {
        pub is_claimable: HashMap<Pubkey, bool>,
        pub is_withdrawable: HashMap<Pubkey, bool>,
        pub referrers: HashMap<Pubkey, Pubkey>,
        pub user_info: HashMap<u64, HashMap<Pubkey, UserInfo>>,
        pub pool_rates: HashMap<u64, HashMap<u64, u64>>,
        pub pool_apys: HashMap<u64, HashMap<u64, u64>>,
        pub governance: Pubkey,
        pub ref_percent: u64, // 200 = 2%
    }
pub fn update_rate(ctx: Context<UpdateRate>, rate: u64) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let current_timestamp = Clock::get()?.unix_timestamp;
    let start_of_day = DateHelper::get_start_of_date(current_timestamp);
    
    let state = &mut ctx.accounts.state;
    
    // Update pool rate for current day
    if let Some(pool_rates) = state.pool_rates.get_mut(&pool.key().to_bytes()) {
        pool_rates.insert(start_of_day as u64, rate);
    } else {
        let mut rates = HashMap::new();
        rates.insert(start_of_day as u64, rate);
        state.pool_rates.insert(pool.key().to_bytes(), rates);
    }

    // Update last rate
    pool.last_rate = rate;

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateRate<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"pool", authority.key().as_ref()],
        bump = pool.bump,
    )]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub state: Account<'info, DextraState>,
}
pub fn update_apy(ctx: Context<UpdateApy>, apy: u64) -> Result<()> {
    // Verify authority is owner or governance
    require!(
        ctx.accounts.authority.key() == OWNER_PUBKEY || 
        ctx.accounts.authority.key() == GOVERNANCE_PUBKEY,
        CustomError::UnauthorizedAccess
    );

    // Validate APY is within reasonable bounds (e.g., 0-10000 for 0-100%)
    require!(apy <= 10000, CustomError::InvalidAPY);

    let pool = &mut ctx.accounts.pool;
    let current_timestamp = Clock::get()?.unix_timestamp;
    let start_of_day = DateHelper::get_start_of_date(current_timestamp);
    
    let state = &mut ctx.accounts.state;
    
    // Update pool apy for current day
    if let Some(pool_apys) = state.pool_apys.get_mut(&pool.key().to_bytes()) {
        pool_apys.insert(start_of_day as u64, apy);
    } else {
        let mut apys = HashMap::new();
        apys.insert(start_of_day as u64, apy);
        state.pool_apys.insert(pool.key().to_bytes(), apys);
    }

    // Update last apy
    pool.last_apy = apy;

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateApy<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"pool", authority.key().as_ref()],
        bump = pool.bump,
    )]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub state: Account<'info, DextraState>,
}
#[account]
#[derive(Default, Copy)]
pub struct PoolInfo {
    pub deposit_token: Pubkey,    // Token mint address for deposit token
    pub reward_token: Pubkey,     // Token mint address for reward token
    pub minimum_deposit: u64,
    pub lock_period: u64,
    pub can_swap: bool,
    pub last_rate: u64,
    pub last_apy: u64,
}

#[account]
pub struct DateHelper {}

impl DateHelper {
    pub const SECONDS_PER_DAY: i64 = 86400; // 24 * 60 * 60

    pub fn get_start_of_date(timestamp: i64) -> i64 {
        (timestamp / Self::SECONDS_PER_DAY) * Self::SECONDS_PER_DAY
    }

    pub fn get_end_of_date(timestamp: i64) -> i64 {
        (timestamp / Self::SECONDS_PER_DAY) * Self::SECONDS_PER_DAY + Self::SECONDS_PER_DAY - 1
    }

    pub fn get_diff_days(timestamp1: i64, timestamp2: i64) -> i64 {
        (timestamp1 - timestamp2) / Self::SECONDS_PER_DAY
    }
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
}

impl UserInfo {
    pub fn new() -> Self {
        Self {
            amount: 0,
            pending_reward: 0,
            last_claimed: 0,
            total_claimed: 0,
            stake_timestamp: 0,
            deposits: Vec::new(),
        }
    }
}

impl DextraState {
    pub const REF_PERCENT: u64 = 200; // 2%
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserDeposit {
    pub amount: u64,
    pub timestamp: i64,
    pub locked_until: i64,
    pub is_withdrawn: bool,
}


#[derive(Accounts)]
#[derive(Accounts)]
    pub struct AddPool<'info> {
        #[account(mut)]
        pub authority: Signer<'info>,
        #[account(
            init,
            payer = authority,
            space = 8 + std::mem::size_of::<Pool>()
        )]
        pub pool: Account<'info, Pool>,
        pub system_program: Program<'info, System>,
    }




#[derive(Accounts)]
pub struct UpdatePool<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"pool", authority.key().as_ref()],
        bump = pool.bump,
    )]
    pub pool: Account<'info, Pool>,
}


#[error_code]
pub enum DextraError {
    #[msg("Deposit amount is less than minimum required")]
    InsufficientDeposit,
}
impl PoolInfo {
    pub const LEN: usize = 32 + 32 + 8 + 8 + 1 + 8 + 8;
}
impl Pool {
    pub const LEN: usize = 8 + PoolInfo::LEN;
}

impl UserInfo {
    pub const LEN: usize = 8 + 8 + 8 + 8 + 8 + 200; // 200 for vec storage
}
#[error_code]
pub enum TransferHelperError {
    #[msg("Transfer failed")]
    TransferFailed,
    #[msg("Approve failed")] 
    ApproveFailed,
    #[msg("SOL transfer failed")]
    SolTransferFailed,
}
#[error_code]
pub enum CustomError {
    #[msg("Unauthorized access")]
    UnauthorizedAccess,
}
#[derive(Accounts)]
pub struct MassCall<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: This account just stores the governance address
    pub governance: AccountInfo<'info>,
}
#[error_code]
pub enum TransferError {
    #[msg("Transfer failed")]
    TransferFailed,
}


// Access control modifier as a function
fn verify_authority(authority: &Pubkey) -> Result<()> {
    require!(
        *authority == OWNER_PUBKEY || *authority == GOVERNANCE_PUBKEY,
        CustomError::UnauthorizedAccess
    );
    Ok(())
}

#[error_code]
pub enum ErrorCode {
    #[msg("Math overflow error")]
    MathOverflow,
}

pub const REF_PERCENT: u64 = 200; // 2%

fn process_ref<'info>(
    pid: u64,
    amount: u64,
    user: Pubkey,
    referrers: &Account<'info, Referrers>,
    pool_info: &mut Account<'info, PoolInfo>,
    token_program: &AccountInfo<'info>,
    from: &Account<'info, TokenAccount>,
    authority: &Signer<'info>,
) -> Result<()> {
    if referrers.get_referrer(user) != Pubkey::default() {
        let ref_amount = (amount.checked_mul(REF_PERCENT)?)
            .checked_div(10000)
            .ok_or(ErrorCode::MathOverflow)?;
            
        safe_send_from_pool(
            pid,
            referrers.get_referrer(user).to_account_info(),
            ref_amount,
            true,
            pool_info,
            token_program,
            from,
            authority,
        )?;
    }
    Ok(())
}

    fn safe_send_from_pool<'info>(
        pid: u64,
        to: AccountInfo<'info>,
        amount: u64,
        is_claim: bool,
        pool_info: &mut Account<'info, PoolInfo>,
        token_program: &AccountInfo<'info>,
        from: &Account<'info, TokenAccount>,
        authority: &Signer<'info>,
    ) -> Result<()> {
        let token = if is_claim {
            pool_info.reward_token
        } else {
            pool_info.deposit_token
        };

        if token == Pubkey::default() {
            safe_transfer_sol(&to, amount)
        } else {
            safe_transfer(
                token_program,
                from, 
                &Account::<TokenAccount>::try_from(&to)?,
                authority,
                amount,
            )
        }
    }

    fn safe_transfer_sol<'info>(
        to: &AccountInfo<'info>,
        value: u64,
    ) -> Result<()> {
        let ix = solana_program::system_instruction::transfer(
            &authority.key(),
            &to.key(),
            value
        );

        solana_program::program::invoke(
            &ix,
            &[
                authority.to_account_info(),
                to.clone(),
            ],
        ).map_err(|_| error!(TransferError::TransferFailed))
    }

    pub fn calculate_reward(pid: u64, user: Pubkey, state: &DextraState) -> Result<u64> {
        if let Some(user_pools) = state.user_info.get(&pid) {
            if let Some(user_info) = user_pools.get(&user) {
                let amount = user_info.amount;
                let last_claimed = user_info.last_claimed;
                let total_reward = user_info.pending_reward;

                if amount == 0 || last_claimed == 0 {
                    return Ok(total_reward);
                }

                let start_timestamp = DateHelper::get_start_of_date(last_claimed);
                let current_timestamp = Clock::get()?.unix_timestamp;
                let mut total_time_reward: u64 = 0;
                let mut last_claimed_temp = last_claimed;

                let mut timestamp = start_timestamp;
                while timestamp < current_timestamp {
                    let apy = state.pool_apys
                        .get(&pid)
                        .and_then(|apys| apys.get(&(timestamp as u64)))
                        .copied()
                        .unwrap_or_else(|| pool.last_apy);

                    let rate = state.pool_rates
                        .get(&pid)
                        .and_then(|rates| rates.get(&(timestamp as u64)))
                        .copied()
                        .unwrap_or_else(|| pool.last_rate);

                    let end_day = timestamp + DateHelper::SECONDS_PER_DAY;
                    let applicable_time = if end_day > current_timestamp {
                        current_timestamp - last_claimed_temp
                    } else {
                        end_day - last_claimed_temp
                    };

                    let yield_amount = amount
                        .checked_mul(applicable_time as u64)?
                        .checked_mul(apy)?
                        .checked_div(36500 * 86400 * 100)?;

                    total_time_reward = total_time_reward
                        .checked_add(yield_amount.checked_mul(rate)?.checked_div(1000000)?)?;

                    last_claimed_temp = if end_day > current_timestamp {
                        current_timestamp
                    } else {
                        end_day
                    };

                    timestamp += DateHelper::SECONDS_PER_DAY;
                }

                // Handle token decimals adjustment
                let deposit_decimals = get_token_decimals(pool.deposit_token)?;
                let reward_decimals = get_token_decimals(pool.reward_token)?;
                
                let final_reward = if reward_decimals >= deposit_decimals {
                    total_time_reward.checked_mul(10u64.pow((reward_decimals - deposit_decimals) as u32))?
                } else {
                    total_time_reward.checked_div(10u64.pow((deposit_decimals - reward_decimals) as u32))?
                };

                Ok(total_reward.checked_add(final_reward)?)
            } else {
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }

    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        // Verify claim is allowed
        let state = &ctx.accounts.state;
        require!(
            state.is_claimable.get(&ctx.accounts.user.key()).copied().unwrap_or(false),
            DextraError::ClaimNotAllowed
        );

        // Calculate reward
        let reward = calculate_reward(
            ctx.accounts.pool.key().to_bytes(),
            ctx.accounts.user.key(),
            state
        )?;

        // Transfer reward tokens
        safe_transfer(
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.pool_token,
            &ctx.accounts.user_token,
            &ctx.accounts.authority,
            reward
        )?;

        // Update user info
        let user = &mut ctx.accounts.user_account;
        user.pending_reward = 0;
        user.last_claimed = Clock::get()?.unix_timestamp;
        user.total_claimed = user.total_claimed.checked_add(reward).unwrap();

        // Process referral reward
        process_ref(
            ctx.accounts.pool.key().to_bytes(),
            reward,
            ctx.accounts.user.key(),
            &ctx.accounts.referrers,
            &mut ctx.accounts.pool,
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.pool_token,
            &ctx.accounts.authority,
        )?;

        emit!(ClaimEvent {
            user: ctx.accounts.user.key(),
            pool: ctx.accounts.pool.key(),
            amount: reward
        });

        Ok(())
    }

    #[derive(Accounts)]
    pub struct Claim<'info> {
        #[account(mut)]
        pub pool: Account<'info, Pool>,
        #[account(mut)]
        pub user: Signer<'info>,
        #[account(mut)]
        pub authority: Signer<'info>,
        #[account(mut)]
        pub user_account: Account<'info, UserInfo>,
        #[account(mut)]
        pub user_token: Account<'info, TokenAccount>,
        #[account(mut)]
        pub pool_token: Account<'info, TokenAccount>,
        pub token_program: Program<'info, Token>,
        pub state: Account<'info, DextraState>,
        pub referrers: Account<'info, Referrers>,
    }

    #[event]
    pub struct ClaimEvent {
        pub user: Pubkey,
        pub pool: Pubkey,
        pub amount: u64,
    }