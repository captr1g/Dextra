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

    pub fn update_pool(
        ctx: Context<UpdatePool>,
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
    pub fn approve(ctx: Context<Approve>, approval_type: u8) -> Result<()> {
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

pub fn masscall(ctx: Context<MassCall>, setup_data: Vec<u8>) -> Result<()> {
    // Verify authority is owner or governance
    require!(
        ctx.accounts.authority.key() == OWNER_PUBKEY || 
        ctx.accounts.authority.key() == GOVERNANCE_PUBKEY,
        CustomError::UnauthorizedAccess
    );

    // Update governance address and execute call
    let state = &mut ctx.accounts.state;
    state.governance = ctx.accounts.governance.key();
    state.owner = ctx.accounts.authority.key();

    Ok(())
}


    // Access control modifier as a function
    fn verify_authority(authority: &Pubkey) -> Result<()> {
        require!(
            *authority == OWNER_PUBKEY || *authority == GOVERNANCE_PUBKEY,
            CustomError::UnauthorizedAccess
        );
        Ok(())
    }

    // Function implementations with authority checks
    pub fn update_rate(ctx: Context<UpdateRate>, rate: u64) -> Result<()> {
        verify_authority(&ctx.accounts.authority.key())?;
        
        let pool = &mut ctx.accounts.pool;
        let current_timestamp = Clock::get()?.unix_timestamp;
        let start_of_day = DateHelper::get_start_of_date(current_timestamp);
        
        pool.last_rate = rate;
        ctx.accounts.state.pool_rates.insert(start_of_day as u64, rate);
        
        Ok(())
    }

    pub fn update_apy(ctx: Context<UpdateApy>, apy: u64) -> Result<()> {
        verify_authority(&ctx.accounts.authority.key())?;
        
        let pool = &mut ctx.accounts.pool;
        let current_timestamp = Clock::get()?.unix_timestamp;
        let start_of_day = DateHelper::get_start_of_date(current_timestamp);
        
        pool.last_apy = apy;
        ctx.accounts.state.pool_apys.insert(start_of_day as u64, apy);
        
        Ok(())
    }

    pub fn update_pool(ctx: Context<UpdatePool>, minimum_deposit: u64, lock_period: i64, can_swap: bool) -> Result<()> {
        verify_authority(&ctx.accounts.authority.key())?;
        
        let pool = &mut ctx.accounts.pool;
        pool.minimum_deposit = minimum_deposit;
        pool.lock_period = lock_period as u64;
        pool.can_swap = can_swap;
        
        Ok(())
    }

    pub fn approve(ctx: Context<Approve>, approval_type: u8) -> Result<()> {
        verify_authority(&ctx.accounts.authority.key())?;
        
        let state = &mut ctx.accounts.state;
        let user = ctx.accounts.user.key();
        
        match approval_type {
            0 | 2 => state.is_claimable.insert(user, true),
            1 => state.is_withdrawable.insert(user, true),
            _ => return Err(CustomError::InvalidApprovalType.into())
        };
        
        Ok(())
    }

    Ok(())
