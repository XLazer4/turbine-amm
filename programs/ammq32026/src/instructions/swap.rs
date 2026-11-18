use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Mint, Token, TokenAccount, Transfer};

use crate::{curve, error::AmmError, state::Config};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    pub mint_x: Box<Account<'info, Mint>>,
    pub mint_y: Box<Account<'info, Mint>>,
    #[account(
        has_one = mint_x,
        has_one = mint_y,
        seeds = [b"config", config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config,
    )]
    pub vault_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config,
    )]
    pub vault_y: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = user,
    )]
    pub user_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = user,
    )]
    pub user_y: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    // pub system_program: Program<'info, System>,
    // pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Swap<'info> {
    pub fn swap(&mut self, is_x: bool, amount: u64, min: u64) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);
        require!(amount > 0, AmmError::InvalidAmount);
        let (reserve_in, reserve_out) = match is_x {
            true => (self.vault_x.amount, self.vault_y.amount),
            false => (self.vault_y.amount, self.vault_x.amount),
        };

        let amount_out = curve::swap_amount_out(reserve_in, reserve_out, amount, self.config.fee)?;
        require!(amount_out > 0, AmmError::InvalidAmount);
        require!(amount_out >= min, AmmError::SlippageExceeded);

        self.deposit_tokens(is_x, amount)?;
        self.withdraw_tokens(is_x, amount_out)
    }

    pub fn deposit_tokens(&mut self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = match is_x {
            true => (
                self.user_x.to_account_info(),
                self.vault_x.to_account_info(),
            ),
            false => (
                self.user_y.to_account_info(),
                self.vault_y.to_account_info(),
            ),
        };

        transfer(
            CpiContext::new(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.user.to_account_info(),
                },
            ),
            amount,
        )
    }

    pub fn withdraw_tokens(&mut self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = match is_x {
            true => (
                self.vault_y.to_account_info(),
                self.user_y.to_account_info(),
            ),
            false => (
                self.vault_x.to_account_info(),
                self.user_x.to_account_info(),
            ),
        };

        transfer(
            CpiContext::new_with_signer(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.config.to_account_info(),
                },
                &[&[
                    b"config",
                    &self.config.seed.to_le_bytes(),
                    &[self.config.config_bump],
                ]],
            ),
            amount,
        )
    }
}
