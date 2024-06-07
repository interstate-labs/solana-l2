use anchor_lang::prelude::*;

pub mod bridge;
pub mod error;

declare_id!("6BbPLThKL7envn7y3TkouLuBqF6qQiXvgLDFFyKzdCM9");

#[program]
pub mod on_chain {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
