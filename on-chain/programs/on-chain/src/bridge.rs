use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use std::collections::BTreeMap;

use crate::error::ErrorCode;

#[account]
pub struct BridgeAccount {
    pub total_deposits: u64,
    pub balances: BTreeMap<Pubkey, u64>,
}

#[event]
pub struct DepositEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct WithdrawEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 8 + 8 + (32 * 1000) + (8 * 1000))] // TODO: Adjust space allocation as needed
    pub bridge_account: Account<'info, BridgeAccount>,

    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
    let bridge_account = &mut ctx.accounts.bridge_account;
    bridge_account.total_deposits = 0;
    bridge_account.balances = BTreeMap::new();
    Ok(())
}


#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub bridge_account: Account<'info, BridgeAccount>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub bridge_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}


pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    let bridge_account = &mut ctx.accounts.bridge_account;

    // Transfer tokens to the bridge account
    let cpi_accounts = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.bridge_token_account.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    token::transfer(cpi_ctx, amount)?;

    // Record the user's deposit
    bridge_account.total_deposits += amount;
    let user_key = ctx.accounts.user.key();
    let balance = bridge_account.balances.entry(user_key).or_insert(0);
    *balance += amount;

    // Emit an event to indicate a deposit
    emit!(DepositEvent {
        user: *ctx.accounts.user.key,
        amount,
        timestamp: Clock::get().unwrap().unix_timestamp,
    });

    Ok(())
}


#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub bridge_account: Account<'info, BridgeAccount>,
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub bridge_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}
pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    let bridge_account = &mut ctx.accounts.bridge_account;

    // Ensure the user has enough balance
    let user_key = &ctx.accounts.user.key();
    let balance = bridge_account.balances.get_mut(user_key).ok_or(ErrorCode::Unauthorized)?;
    require!(*balance >= amount, ErrorCode::InsufficientFunds);

    // Update user's balance
    *balance -= amount;

    // Transfer tokens from the bridge account back to the user
    let cpi_accounts = Transfer {
        from: ctx.accounts.bridge_token_account.to_account_info(),
        to: ctx.accounts.user_token_account.to_account_info(),
        authority: ctx.accounts.bridge_account.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    token::transfer(cpi_ctx, amount)?;

    // Emit an event to indicate a withdrawal
    emit!(WithdrawEvent {
        user: *ctx.accounts.user.key,
        amount,
        timestamp: Clock::get().unwrap().unix_timestamp,
    });

    Ok(())
}

