pub mod curve;
pub mod error;
pub mod fees;

use crate::curve::{ConstantProduct, TradeDirection};
use crate::fees::{Fees, FIXED_CONSTRAINTS};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token::{self, Burn, Mint, MintTo, TokenAccount, Transfer};

declare_id!("HEnMwtqH2T6bVHGwTkbbj2WBKJs6G4TztVSeUC9w1Tb1");

#[program]
pub mod anchor_liquidity_pool {
    use super::*;
    use crate::curve::{RoundDirection, TradeDirection};

    pub fn initialize(ctx: Context<Initialize>, fees: Fees) -> Result<()> {
        msg!("Instruction Pool Init {:?}", fees);
        let curve = ConstantProduct {};
        curve.validate_supply(
            ctx.accounts.token_a_for_pda.amount,
            ctx.accounts.token_b_for_pda.amount,
        )?;
        FIXED_CONSTRAINTS.validate_fees(&fees)?;
        fees.validate()?;

        ctx.accounts.pair.token_a_account = ctx.accounts.token_a_for_pda.key();
        ctx.accounts.pair.token_b_account = ctx.accounts.token_b_for_pda.key();
        ctx.accounts.pair.pool_mint = ctx.accounts.pool.key();
        ctx.accounts.pair.pool_fee_account = ctx.accounts.token_pool_for_fee_receiver.key();
        ctx.accounts.pair.token_a_mint = ctx.accounts.token_a_for_pda.mint.key();
        ctx.accounts.pair.token_b_mint = ctx.accounts.token_b_for_pda.mint.key();
        ctx.accounts.pair.fees = fees;

        let signer_seeds = ctx
            .accounts
            .pair
            .signer_seeds(&ctx.accounts.pda, ctx.program_id)?;
        let signer_seeds = &[&signer_seeds.value()[..]];
        token::mint_to(
            ctx.accounts.to_mint_context().with_signer(signer_seeds),
            to_u64(curve.new_pool_supply())?,
        )?;
        Ok(())
    }

    pub fn deposit_all(
        ctx: Context<DepositAll>,
        pool_token_amount: u64,
        maximum_token_a_amount: u64,
        maximum_token_b_amount: u64,
    ) -> Result<()> {
        msg!(
            "Instruction Pool Deposit {},{},{}",
            pool_token_amount,
            maximum_token_a_amount,
            maximum_token_b_amount
        );

        let curve = ConstantProduct {};
        let current_pool_mint_supply = to_u128(ctx.accounts.pool.supply)?;
        let (pool_token_amount, pool_mint_supply) = if current_pool_mint_supply > 0 {
            (to_u128(pool_token_amount)?, current_pool_mint_supply)
        } else {
            (curve.new_pool_supply(), curve.new_pool_supply())
        };

        let (token_a_amount, token_b_amount) = curve
            .pool_tokens_to_trading_tokens(
                pool_token_amount,
                pool_mint_supply,
                to_u128(ctx.accounts.token_a_for_pda.amount)?,
                to_u128(ctx.accounts.token_b_for_pda.amount)?,
                RoundDirection::Ceiling,
            )
            .ok_or(crate::error::Error::ZeroTradingTokens)?;

        msg!(
            "Pooling token amount of A is {} and B is {}",
            token_a_amount,
            token_b_amount
        );
        let token_a_amount = to_u64(token_a_amount)?;
        if token_a_amount > maximum_token_a_amount {
            return Err(crate::error::Error::ExceededSlippage.into());
        }
        if token_a_amount == 0 {
            return Err(crate::error::Error::ZeroTradingTokens.into());
        }
        let token_b_amount = to_u64(token_b_amount)?;
        if token_b_amount > maximum_token_b_amount {
            return Err(crate::error::Error::ExceededSlippage.into());
        }
        if token_b_amount == 0 {
            return Err(crate::error::Error::ZeroTradingTokens.into());
        }

        let signer_seeds = ctx
            .accounts
            .pair
            .signer_seeds(&ctx.accounts.pda, ctx.program_id)?;
        let signer_seeds = &[&signer_seeds.value()[..]];

        token::transfer(ctx.accounts.to_transfer_a_context(), token_a_amount)?;
        token::transfer(ctx.accounts.to_transfer_b_context(), token_b_amount)?;
        token::mint_to(
            ctx.accounts.to_mint_context().with_signer(signer_seeds),
            to_u64(pool_token_amount)?,
        )?;

        Ok(())
    }

    pub fn deposit_single(
        ctx: Context<DepositSingle>,
        source_token_amount: u64,
        minimum_pool_token_amount: u64,
    ) -> Result<()> {
        msg!(
            "Instruction Pool Deposit Single {},{}",
            source_token_amount,
            minimum_pool_token_amount,
        );

        let trade_direction = ctx.accounts.trade_direction()?;
        let curve = ConstantProduct {};
        let pool_token_amount = if ctx.accounts.pool.supply > 0 {
            curve
                .deposit_single_token_type(
                    to_u128(source_token_amount)?,
                    to_u128(ctx.accounts.token_a_for_pda.amount)?,
                    to_u128(ctx.accounts.token_b_for_pda.amount)?,
                    to_u128(ctx.accounts.pool.supply)?,
                    &trade_direction,
                    &ctx.accounts.pair.fees,
                )
                .ok_or(crate::error::Error::ZeroTradingTokens)?
        } else {
            curve.new_pool_supply()
        };

        let pool_token_amount = to_u64(pool_token_amount)?;
        if pool_token_amount < minimum_pool_token_amount {
            return Err(crate::error::Error::ExceededSlippage.into());
        }
        if pool_token_amount == 0 {
            return Err(crate::error::Error::ZeroTradingTokens.into());
        }

        let signer_seeds = ctx
            .accounts
            .pair
            .signer_seeds(&ctx.accounts.pda, ctx.program_id)?;
        let signer_seeds = &[&signer_seeds.value()[..]];
        token::transfer(
            ctx.accounts.to_transfer_context(trade_direction),
            source_token_amount,
        )?;
        token::mint_to(
            ctx.accounts.to_mint_context().with_signer(signer_seeds),
            pool_token_amount,
        )?;

        Ok(())
    }

    pub fn withdraw_all(
        ctx: Context<WithdrawAll>,
        pool_token_amount: u64,
        minimum_token_a_amount: u64,
        minimum_token_b_amount: u64,
    ) -> Result<()> {
        msg!(
            "Instruction Pool Withdraw {},{},{}",
            pool_token_amount,
            minimum_token_a_amount,
            minimum_token_b_amount
        );

        let curve = ConstantProduct {};
        let withdraw_fee = ctx.accounts.withdraw_fee(pool_token_amount)?;

        let pool_token_amount = to_u128(pool_token_amount)?
            .checked_sub(withdraw_fee)
            .ok_or(crate::error::Error::FeeCalculationFailure)?;

        let (token_a_amount, token_b_amount) = curve
            .pool_tokens_to_trading_tokens(
                pool_token_amount,
                to_u128(ctx.accounts.pool.supply)?,
                to_u128(ctx.accounts.token_a_for_pda.amount)?,
                to_u128(ctx.accounts.token_b_for_pda.amount)?,
                RoundDirection::Floor,
            )
            .ok_or(crate::error::Error::ZeroTradingTokens)?;

        let token_a_amount =
            std::cmp::min(ctx.accounts.token_a_for_pda.amount, to_u64(token_a_amount)?);
        msg!(
            "pool_token_amount={}, token_a_amount={}, token_b_amount={}, withdraw_fee={}",
            pool_token_amount,
            token_a_amount,
            token_b_amount,
            withdraw_fee
        );

        if token_a_amount < minimum_token_a_amount {
            return Err(crate::error::Error::ExceededSlippage.into());
        }
        if token_a_amount == 0 && ctx.accounts.token_a_for_pda.amount != 0 {
            return Err(crate::error::Error::ZeroTradingTokens.into());
        }
        let token_b_amount =
            std::cmp::min(ctx.accounts.token_b_for_pda.amount, to_u64(token_b_amount)?);
        if token_b_amount < minimum_token_b_amount {
            return Err(crate::error::Error::ExceededSlippage.into());
        }
        if token_b_amount == 0 && ctx.accounts.token_b_for_pda.amount != 0 {
            return Err(crate::error::Error::ZeroTradingTokens.into());
        }

        if withdraw_fee > 0 {
            token::transfer(
                ctx.accounts.to_transfer_fee_context(),
                to_u64(withdraw_fee)?,
            )?
        }
        token::burn(ctx.accounts.to_burn_context(), to_u64(pool_token_amount)?)?;

        let signer_seeds = ctx
            .accounts
            .pair
            .signer_seeds(&ctx.accounts.pda, ctx.program_id)?;
        let signer_seeds = &[&signer_seeds.value()[..]];
        if token_a_amount > 0 {
            token::transfer(
                ctx.accounts
                    .to_transfer_a_context()
                    .with_signer(signer_seeds),
                token_a_amount,
            )?;
        }
        if token_b_amount > 0 {
            token::transfer(
                ctx.accounts
                    .to_transfer_b_context()
                    .with_signer(signer_seeds),
                token_b_amount,
            )?;
        }

        Ok(())
    }

    pub fn withdraw_single(
        ctx: Context<WithdrawSingle>,
        destination_token_amount: u64,
        maximum_pool_token_amount: u64,
    ) -> Result<()> {
        msg!(
            "Instruction Pool Withdraw Single {},{}",
            destination_token_amount,
            maximum_pool_token_amount
        );

        let trade_direction = ctx.accounts.trade_direction()?;
        let curve = ConstantProduct {};
        let burn_pool_token_amount = curve
            .withdraw_single_token_type_exact_out(
                to_u128(destination_token_amount)?,
                to_u128(ctx.accounts.token_a_for_pda.amount)?,
                to_u128(ctx.accounts.token_b_for_pda.amount)?,
                to_u128(ctx.accounts.pool.supply)?,
                &trade_direction,
                &ctx.accounts.pair.fees,
            )
            .ok_or(crate::error::Error::ZeroTradingTokens)?;

        let withdraw_fee = ctx.accounts.withdraw_fee(burn_pool_token_amount)?;
        let pool_token_amount = burn_pool_token_amount
            .checked_add(withdraw_fee)
            .ok_or(crate::error::Error::CalculationFailure)?;
        msg!(
            "pool_token_amount={}, burn_token_amount={}, withdraw_fee={}",
            pool_token_amount,
            burn_pool_token_amount,
            withdraw_fee
        );

        let pool_token_amount = to_u64(pool_token_amount)?;
        if pool_token_amount > maximum_pool_token_amount {
            return Err(crate::error::Error::ExceededSlippage.into());
        }
        if pool_token_amount == 0 {
            return Err(crate::error::Error::ZeroTradingTokens.into());
        }

        if withdraw_fee > 0 {
            token::transfer(
                ctx.accounts.to_transfer_fee_context(),
                to_u64(withdraw_fee)?,
            )?
        }
        token::burn(
            ctx.accounts.to_burn_context(),
            to_u64(burn_pool_token_amount)?,
        )?;

        let signer_seeds = ctx
            .accounts
            .pair
            .signer_seeds(&ctx.accounts.pda, ctx.program_id)?;
        let signer_seeds = &[&signer_seeds.value()[..]];
        token::transfer(
            ctx.accounts
                .to_transfer_context(trade_direction)
                .with_signer(signer_seeds),
            destination_token_amount,
        )?;

        Ok(())
    }

    pub fn swap(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<()> {
        msg!("Instruction Swap {},{}", amount_in, minimum_amount_out,);

        let trade_direction = ctx.accounts.trade_direction();
        let curve = ConstantProduct {};
        let result = curve
            .swap(
                to_u128(amount_in)?,
                to_u128(ctx.accounts.token_source_for_pda.amount)?,
                to_u128(ctx.accounts.token_destination_for_pda.amount)?,
                &ctx.accounts.pair.fees,
            )
            .ok_or(crate::error::Error::ZeroTradingTokens)?;

        msg!("{:?}", result);
        if result.destination_amount_swapped < to_u128(minimum_amount_out)? {
            return Err(crate::error::Error::ExceededSlippage.into());
        }

        let (swap_token_a_amount, swap_token_b_amount) = match trade_direction {
            TradeDirection::AtoB => (
                result.new_swap_source_amount,
                result.new_swap_destination_amount,
            ),
            TradeDirection::BtoA => (
                result.new_swap_destination_amount,
                result.new_swap_source_amount,
            ),
        };

        let mut pool_token_amount = curve
            .withdraw_single_token_type_exact_out(
                result.owner_fee,
                swap_token_a_amount,
                swap_token_b_amount,
                to_u128(ctx.accounts.pool.supply)?,
                &trade_direction,
                &ctx.accounts.pair.fees,
            )
            .ok_or(crate::error::Error::FeeCalculationFailure)?;

        msg!("pool_token_amount={}", pool_token_amount);
        let signer_seeds = ctx
            .accounts
            .pair
            .signer_seeds(&ctx.accounts.pda, ctx.program_id)?;
        let signer_seeds = &[&signer_seeds.value()[..]];

        if pool_token_amount > 0 {
            let host_fee = ctx
                .accounts
                .pair
                .fees
                .host_fee(pool_token_amount)
                .ok_or(crate::error::Error::FeeCalculationFailure)?;
            if host_fee > 0 {
                token::mint_to(
                    ctx.accounts
                        .to_mint_host_fee_context()
                        .with_signer(signer_seeds),
                    to_u64(host_fee)?,
                )?;
                pool_token_amount = pool_token_amount
                    .checked_sub(host_fee)
                    .ok_or(crate::error::Error::FeeCalculationFailure)?;
            }
            token::mint_to(
                ctx.accounts
                    .to_mint_pool_fee_context()
                    .with_signer(signer_seeds),
                to_u64(pool_token_amount)?,
            )?;
        }

        token::transfer(
            ctx.accounts.to_transfer_source_context(),
            to_u64(result.source_amount_swapped)?,
        )?;
        token::transfer(
            ctx.accounts
                .to_transfer_destination_context()
                .with_signer(signer_seeds),
            to_u64(result.destination_amount_swapped)?,
        )?;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(fees: Fees)]
pub struct Initialize<'info> {
    #[account(zero)]
    pub pair: Box<Account<'info, SwapPair>>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub pda: AccountInfo<'info>,
    #[account(
        mut,
        constraint = pool.mint_authority == COption::Some(pda.key()),
        constraint = pool.freeze_authority.is_none()
    )]
    pub pool: Account<'info, Mint>,
    #[account(
        mut,
        constraint = token_a_for_pda.mint != pool.key(),
        constraint = token_a_for_pda.owner == pda.key(),
        constraint = token_a_for_pda.delegate.is_none(),
        constraint = token_a_for_pda.close_authority.is_none()
    )]
    pub token_a_for_pda: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = token_b_for_pda.mint != pool.key(),
        constraint = token_b_for_pda.owner == pda.key(),
        constraint = token_b_for_pda.delegate.is_none(),
        constraint = token_b_for_pda.close_authority.is_none()
    )]
    pub token_b_for_pda: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = token_pool_for_initializer.mint == token_pool_for_fee_receiver.mint,
        constraint = token_pool_for_initializer.owner == token_pool_for_fee_receiver.owner
    )]
    pub token_pool_for_initializer: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = token_pool_for_fee_receiver.mint == pool.key(),
        constraint = token_pool_for_fee_receiver.owner == admin_pubkey()?
    )]
    pub token_pool_for_fee_receiver: Account<'info, TokenAccount>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(pool_token_amount: u64, maximum_token_a_amount: u64, maximum_token_b_amount: u64)]
pub struct DepositAll<'info> {
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub depositor: Signer<'info>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub pda: AccountInfo<'info>,

    #[account(
        mut,
        constraint = pair.token_a_account == token_a_for_pda.key(),
        constraint = pair.token_b_account == token_b_for_pda.key(),
        constraint = pair.token_a_mint == token_a_for_depositor.mint,
        constraint = pair.token_b_mint == token_b_for_depositor.mint,
        constraint = pair.token_a_mint == token_a_for_pda.mint,
        constraint = pair.token_b_mint == token_b_for_pda.mint,
        constraint = pair.pool_mint == pool.key()
    )]
    pub pair: Box<Account<'info, SwapPair>>,

    #[account(
        mut,
        constraint = pool.mint_authority == COption::Some(pda.key()),
        constraint = pool.freeze_authority.is_none()
    )]
    pub pool: Account<'info, Mint>,

    #[account(
        mut,
        constraint = token_a_for_depositor.owner == depositor.key(),
        constraint = token_a_for_depositor.delegate.is_none(),
        constraint = token_a_for_depositor.close_authority.is_none(),
    )]
    pub token_a_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_b_for_depositor.owner == depositor.key(),
        constraint = token_b_for_depositor.delegate.is_none(),
        constraint = token_b_for_depositor.close_authority.is_none(),
    )]
    pub token_b_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_pool_for_depositor.owner == depositor.key(),
        constraint = token_pool_for_depositor.mint == pool.key(),
    )]
    pub token_pool_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_a_for_pda.owner == pda.key(),
        constraint = token_a_for_pda.delegate.is_none(),
        constraint = token_a_for_pda.close_authority.is_none()
    )]
    pub token_a_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_b_for_pda.owner == pda.key(),
        constraint = token_b_for_pda.delegate.is_none(),
        constraint = token_b_for_pda.close_authority.is_none()
    )]
    pub token_b_for_pda: Box<Account<'info, TokenAccount>>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(source_token_amount: u64, mainimum_pool_token_amount: u64)]
pub struct DepositSingle<'info> {
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub depositor: Signer<'info>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub pda: AccountInfo<'info>,

    #[account(
        mut,
        constraint = pair.token_a_account == token_a_for_pda.key(),
        constraint = pair.token_b_account == token_b_for_pda.key(),
        constraint = pair.token_a_mint == token_a_for_pda.mint,
        constraint = pair.token_b_mint == token_b_for_pda.mint,
        constraint = pair.pool_mint == pool.key()
    )]
    pub pair: Box<Account<'info, SwapPair>>,

    #[account(
        mut,
        constraint = pool.mint_authority == COption::Some(pda.key()),
        constraint = pool.freeze_authority.is_none()
    )]
    pub pool: Account<'info, Mint>,

    #[account(
        mut,
        constraint = token_source_for_depositor.owner == depositor.key(),
        constraint = token_source_for_depositor.delegate.is_none(),
        constraint = token_source_for_depositor.close_authority.is_none(),
    )]
    pub token_source_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_pool_for_depositor.owner == depositor.key(),
        constraint = token_pool_for_depositor.mint == pool.key(),
    )]
    pub token_pool_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_a_for_pda.owner == pda.key(),
        constraint = token_a_for_pda.delegate.is_none(),
        constraint = token_a_for_pda.close_authority.is_none()
    )]
    pub token_a_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_b_for_pda.owner == pda.key(),
        constraint = token_b_for_pda.delegate.is_none(),
        constraint = token_b_for_pda.close_authority.is_none()
    )]
    pub token_b_for_pda: Box<Account<'info, TokenAccount>>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(pool_token_amount: u64, minimum_token_a_amount: u64, minimum_token_b_amount: u64)]
pub struct WithdrawAll<'info> {
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub depositor: Signer<'info>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub pda: AccountInfo<'info>,

    #[account(
        mut,
        constraint = pair.token_a_account == token_a_for_pda.key(),
        constraint = pair.token_b_account == token_b_for_pda.key(),
        constraint = pair.token_a_mint == token_a_for_depositor.mint,
        constraint = pair.token_b_mint == token_b_for_depositor.mint,
        constraint = pair.token_a_mint == token_a_for_pda.mint,
        constraint = pair.token_b_mint == token_b_for_pda.mint,
        constraint = pair.pool_mint == pool.key(),
        constraint = pair.pool_fee_account == pool_fee_account.key(),
    )]
    pub pair: Box<Account<'info, SwapPair>>,

    #[account(
        mut,
        constraint = pool.mint_authority == COption::Some(pda.key()),
        constraint = pool.freeze_authority.is_none()
    )]
    pub pool: Account<'info, Mint>,

    #[account(
        mut,
        constraint = token_a_for_depositor.owner == depositor.key(),
        constraint = token_a_for_depositor.delegate.is_none(),
        constraint = token_a_for_depositor.close_authority.is_none(),
    )]
    pub token_a_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_b_for_depositor.owner == depositor.key(),
        constraint = token_b_for_depositor.delegate.is_none(),
        constraint = token_b_for_depositor.close_authority.is_none(),
    )]
    pub token_b_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_pool_for_depositor.owner == depositor.key(),
        constraint = token_pool_for_depositor.mint == pool.key(),
    )]
    pub token_pool_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_a_for_pda.owner == pda.key(),
        constraint = token_a_for_pda.delegate.is_none(),
        constraint = token_a_for_pda.close_authority.is_none()
    )]
    pub token_a_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_b_for_pda.owner == pda.key(),
        constraint = token_b_for_pda.delegate.is_none(),
        constraint = token_b_for_pda.close_authority.is_none()
    )]
    pub token_b_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub pool_fee_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(destination_token_amount: u64, maximum_pool_token_amount: u64)]
pub struct WithdrawSingle<'info> {
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub depositor: Signer<'info>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub pda: AccountInfo<'info>,

    #[account(
        mut,
        constraint = pair.token_a_account == token_a_for_pda.key(),
        constraint = pair.token_b_account == token_b_for_pda.key(),
        constraint = pair.token_a_mint == token_a_for_pda.mint,
        constraint = pair.token_b_mint == token_b_for_pda.mint,
        constraint = pair.pool_mint == pool.key(),
        constraint = pair.pool_fee_account == pool_fee_account.key(),
    )]
    pub pair: Box<Account<'info, SwapPair>>,

    #[account(
        mut,
        constraint = pool.mint_authority == COption::Some(pda.key()),
        constraint = pool.freeze_authority.is_none()
    )]
    pub pool: Account<'info, Mint>,

    #[account(
        mut,
        constraint = token_destination_for_depositor.owner == depositor.key(),
        constraint = token_destination_for_depositor.delegate.is_none(),
        constraint = token_destination_for_depositor.close_authority.is_none(),
    )]
    pub token_destination_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_pool_for_depositor.owner == depositor.key(),
        constraint = token_pool_for_depositor.mint == pool.key(),
    )]
    pub token_pool_for_depositor: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_a_for_pda.owner == pda.key(),
        constraint = token_a_for_pda.delegate.is_none(),
        constraint = token_a_for_pda.close_authority.is_none()
    )]
    pub token_a_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_b_for_pda.owner == pda.key(),
        constraint = token_b_for_pda.delegate.is_none(),
        constraint = token_b_for_pda.close_authority.is_none()
    )]
    pub token_b_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub pool_fee_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(amount_in: u64, minimum_amount_out: u64)]
pub struct Swap<'info> {
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub swapper: Signer<'info>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub pda: AccountInfo<'info>,

    #[account(
        mut,
        constraint = pool.mint_authority == COption::Some(pda.key()),
        constraint = pool.freeze_authority.is_none()
    )]
    pub pool: Account<'info, Mint>,

    #[account(
        mut,
        constraint = pair.token_a_account == token_source_for_pda.key() || pair.token_a_account == token_destination_for_pda.key(),
        constraint = pair.token_b_account == token_source_for_pda.key() || pair.token_b_account == token_destination_for_pda.key(),
        constraint = pair.token_a_mint == token_source_for_pda.mint || pair.token_a_mint == token_destination_for_pda.mint,
        constraint = pair.token_b_mint == token_source_for_pda.mint || pair.token_b_mint == token_destination_for_pda.mint,
        constraint = pair.pool_mint == pool.key(),
        constraint = pair.pool_fee_account == pool_fee_account.key(),
    )]
    pub pair: Box<Account<'info, SwapPair>>,

    #[account(
        mut,
        constraint = token_source_for_swapper.owner == swapper.key(),
        constraint = token_source_for_swapper.delegate.is_none(),
        constraint = token_source_for_swapper.close_authority.is_none()
    )]
    pub token_source_for_swapper: Box<Account<'info, TokenAccount>>,

    #[account(
    mut,
        constraint = token_destination_for_swapper.owner == swapper.key(),
        constraint = token_destination_for_swapper.delegate.is_none(),
        constraint = token_destination_for_swapper.close_authority.is_none()
    )]
    pub token_destination_for_swapper: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_source_for_pda.mint == token_source_for_swapper.mint,
        constraint = token_source_for_pda.owner == pda.key(),
        constraint = token_source_for_pda.delegate.is_none(),
        constraint = token_source_for_pda.close_authority.is_none()
    )]
    pub token_source_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = token_destination_for_pda.mint == token_destination_for_swapper.mint,
        constraint = token_destination_for_pda.owner == pda.key(),
        constraint = token_destination_for_pda.delegate.is_none(),
        constraint = token_destination_for_pda.close_authority.is_none()
    )]
    pub token_destination_for_pda: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = pool_fee_account.mint == pool.key()
    )]
    pub pool_fee_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = host_fee_account.mint == pool.key()
    )]
    pub host_fee_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub token_program: AccountInfo<'info>,
}

impl<'info> Initialize<'info> {
    fn to_mint_context(&self) -> CpiContext<'_, '_, '_, 'info, MintTo<'info>> {
        let cpi_accounts = MintTo {
            mint: self.pool.to_account_info().clone(),
            to: self.token_pool_for_initializer.to_account_info().clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }
}

impl<'info> DepositAll<'info> {
    fn to_transfer_a_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_a_for_depositor.to_account_info().clone(),
            to: self.token_a_for_pda.to_account_info().clone(),
            authority: self.depositor.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_transfer_b_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_b_for_depositor.to_account_info().clone(),
            to: self.token_b_for_pda.to_account_info().clone(),
            authority: self.depositor.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_mint_context(&self) -> CpiContext<'_, '_, '_, 'info, MintTo<'info>> {
        let cpi_accounts = MintTo {
            mint: self.pool.to_account_info().clone(),
            to: self.token_pool_for_depositor.to_account_info().clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }
}

impl<'info> DepositSingle<'info> {
    fn to_transfer_context(
        &self,
        direction: TradeDirection,
    ) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_source_for_depositor.to_account_info().clone(),
            to: match direction {
                TradeDirection::AtoB => self.token_a_for_pda.to_account_info().clone(),
                TradeDirection::BtoA => self.token_b_for_pda.to_account_info().clone(),
            },
            authority: self.depositor.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_mint_context(&self) -> CpiContext<'_, '_, '_, 'info, MintTo<'info>> {
        let cpi_accounts = MintTo {
            mint: self.pool.to_account_info().clone(),
            to: self.token_pool_for_depositor.to_account_info().clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn trade_direction(&self) -> Result<TradeDirection> {
        if self.token_source_for_depositor.mint == self.pair.token_a_mint {
            Ok(TradeDirection::AtoB)
        } else if self.token_source_for_depositor.mint == self.pair.token_b_mint.key() {
            Ok(TradeDirection::BtoA)
        } else {
            return Err(crate::error::Error::IncorrectSwapAccount.into());
        }
    }
}

impl<'info> WithdrawAll<'info> {
    fn to_transfer_fee_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_pool_for_depositor.to_account_info().clone(),
            to: self.pool_fee_account.to_account_info().clone(),
            authority: self.depositor.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_transfer_a_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_a_for_pda.to_account_info().clone(),
            to: self.token_a_for_depositor.to_account_info().clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_transfer_b_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_b_for_pda.to_account_info().clone(),
            to: self.token_b_for_depositor.to_account_info().clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_burn_context(&self) -> CpiContext<'_, '_, '_, 'info, Burn<'info>> {
        let cpi_accounts = Burn {
            mint: self.pool.to_account_info().clone(),
            from: self.token_pool_for_depositor.to_account_info().clone(),
            authority: self.depositor.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn withdraw_fee(&self, pool_token_amount: u64) -> Result<u128> {
        Ok(
            if self.token_pool_for_depositor.key() == self.pair.pool_fee_account {
                // withdrawing from the fee account, don't assess withdraw fee
                0
            } else {
                self.pair
                    .fees
                    .owner_withdraw_fee(to_u128(pool_token_amount)?)
                    .ok_or(crate::error::Error::FeeCalculationFailure)?
            },
        )
    }
}

impl<'info> WithdrawSingle<'info> {
    fn to_transfer_fee_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_pool_for_depositor.to_account_info().clone(),
            to: self.pool_fee_account.to_account_info().clone(),
            authority: self.depositor.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_transfer_context(
        &self,
        direction: TradeDirection,
    ) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: match direction {
                TradeDirection::AtoB => self.token_a_for_pda.to_account_info().clone(),
                TradeDirection::BtoA => self.token_b_for_pda.to_account_info().clone(),
            },
            to: self
                .token_destination_for_depositor
                .to_account_info()
                .clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_burn_context(&self) -> CpiContext<'_, '_, '_, 'info, Burn<'info>> {
        let cpi_accounts = Burn {
            mint: self.pool.to_account_info().clone(),
            from: self.token_pool_for_depositor.to_account_info().clone(),
            authority: self.depositor.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn trade_direction(&self) -> Result<TradeDirection> {
        if self.token_destination_for_depositor.mint == self.pair.token_a_mint {
            Ok(TradeDirection::AtoB)
        } else if self.token_destination_for_depositor.mint == self.pair.token_b_mint.key() {
            Ok(TradeDirection::BtoA)
        } else {
            Err(crate::error::Error::IncorrectSwapAccount.into())
        }
    }

    fn withdraw_fee(&self, burn_pool_token_amount: u128) -> Result<u128> {
        Ok(
            if self.token_pool_for_depositor.key() == self.pair.pool_fee_account {
                // withdrawing from the fee account, don't assess withdraw fee
                0
            } else {
                self.pair
                    .fees
                    .owner_withdraw_fee(burn_pool_token_amount)
                    .ok_or(crate::error::Error::FeeCalculationFailure)?
            },
        )
    }
}

impl<'info> Swap<'info> {
    fn to_mint_pool_fee_context(&self) -> CpiContext<'_, '_, '_, 'info, MintTo<'info>> {
        let cpi_accounts = MintTo {
            mint: self.pool.to_account_info().clone(),
            to: self.pool_fee_account.to_account_info().clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_mint_host_fee_context(&self) -> CpiContext<'_, '_, '_, 'info, MintTo<'info>> {
        let cpi_accounts = MintTo {
            mint: self.pool.to_account_info().clone(),
            to: self.host_fee_account.to_account_info().clone(),
            authority: self.pda.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_transfer_source_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_source_for_swapper.to_account_info().clone(),
            to: self.token_source_for_pda.to_account_info().clone(),
            authority: self.swapper.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn to_transfer_destination_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.token_destination_for_pda.to_account_info().clone(),
            to: self.token_destination_for_swapper.to_account_info().clone(),
            authority: self.pda.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn trade_direction(&self) -> TradeDirection {
        if self.token_source_for_pda.key() == self.pair.token_a_account {
            TradeDirection::AtoB
        } else {
            TradeDirection::BtoA
        }
    }
}

struct SignerSeeds<'a>([&'a [u8]; 3], [u8; 1]);

impl<'a> SignerSeeds<'a> {
    pub fn value(&self) -> [&[u8]; 4] {
        [self.0[0], self.0[1], self.0[2], &self.1]
    }
}
#[account]
pub struct SwapPair {
    pub token_a_account: Pubkey,
    pub token_b_account: Pubkey,
    pub pool_mint: Pubkey,
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub pool_fee_account: Pubkey,
    pub fees: Fees,
}

impl SwapPair {
    fn signer_seeds<'a>(&'a self, pda: &AccountInfo, program_id: &Pubkey) -> Result<SignerSeeds> {
        let seeds = [
            b"pool".as_ref(),
            self.token_a_mint.as_ref(),
            self.token_b_mint.as_ref(),
        ];
        let (pubkey, bump_seed) = Pubkey::find_program_address(&seeds, program_id);
        if pubkey != pda.key() {
            return Err(ProgramError::InvalidArgument.into());
        }
        Ok(SignerSeeds(seeds, [bump_seed]))
    }
}

fn admin_pubkey() -> Result<Pubkey> {
    env!("ADMIN_PUBKEY")
        .parse::<Pubkey>()
        .map_err(|_| crate::error::Error::InvalidOwner.into())
}

fn to_u128(val: u64) -> Result<u128> {
    val.try_into()
        .map_err(|_| crate::error::Error::ConversionFailure.into())
}

fn to_u64(val: u128) -> Result<u64> {
    val.try_into()
        .map_err(|_| crate::error::Error::ConversionFailure.into())
}
