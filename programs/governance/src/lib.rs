use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Governance111111111111111111111111111111111");

#[program]
pub mod governance {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let governance = &mut ctx.accounts.governance;
        governance.authority = ctx.accounts.authority.key();
        governance.counter = 0;
        Ok(())
    }

    // Simple function to increment a counter
    pub fn increment_counter(ctx: Context<UpdateGovernance>) -> Result<()> {
        let governance = &mut ctx.accounts.governance;
        governance.counter += 1;
        msg!("Counter incremented to: {}", governance.counter);
        Ok(())
    }

    // Function to receive tokens
    pub fn receive_tokens(ctx: Context<ReceiveTokens>, amount: u64) -> Result<()> {
        // Transfer tokens from sender to governance token account
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.sender_token_account.to_account_info(),
                    to: ctx.accounts.governance_token_account.to_account_info(),
                    authority: ctx.accounts.sender_authority.to_account_info(),
                },
            ),
            amount,
        )?;

        msg!("Received {} tokens", amount);
        Ok(())
    }

    // Function to send tokens
    pub fn send_tokens(ctx: Context<SendTokens>, amount: u64) -> Result<()> {
        // Get PDA signer seeds
        let governance_key = ctx.accounts.governance.key();
        let seeds = &[b"governance", governance_key.as_ref(), &[ctx.bumps.governance_authority]];
        let signer = &[&seeds[..]];

        // Transfer tokens from governance token account to recipient
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.governance_token_account.to_account_info(),
                    to: ctx.accounts.recipient_token_account.to_account_info(),
                    authority: ctx.accounts.governance_authority.to_account_info(),
                },
                signer,
            ),
            amount,
        )?;

        msg!("Sent {} tokens", amount);
        Ok(())
    }

    // Function that will fail - for testing error handling
    pub fn will_fail(_ctx: Context<UpdateGovernance>) -> Result<()> {
        return err!(GovernanceError::IntentionalFailure);
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8, // discriminator + pubkey + counter
    )]
    pub governance: Account<'info, GovernanceState>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateGovernance<'info> {
    #[account(
        mut,
        constraint = governance.authority == authority.key() @ GovernanceError::Unauthorized
    )]
    pub governance: Account<'info, GovernanceState>,
    
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct ReceiveTokens<'info> {
    pub governance: Account<'info, GovernanceState>,
    
    #[account(mut)]
    pub sender_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub governance_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub sender_authority: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SendTokens<'info> {
    pub governance: Account<'info, GovernanceState>,
    
    #[account(
        seeds = [b"governance", governance.key().as_ref()],
        bump,
    )]
    pub governance_authority: SystemAccount<'info>,
    
    #[account(mut)]
    pub governance_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub recipient_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct GovernanceState {
    pub authority: Pubkey,
    pub counter: u64,
}

#[error_code]
pub enum GovernanceError {
    #[msg("Unauthorized access")]
    Unauthorized,
    
    #[msg("Intentional failure for testing")]
    IntentionalFailure,
} 