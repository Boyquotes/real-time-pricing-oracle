mod state;

use crate::state::UpdateData;
use anchor_lang::prelude::*;
use anchor_lang::prelude::borsh::BorshSchema;
use ephemeral_rollups_sdk::anchor::{commit, delegate, ephemeral};
use ephemeral_rollups_sdk::ephem::commit_and_undelegate_accounts;
use ephemeral_rollups_sdk::utils::close_pda;
use ephemeral_rollups_sdk::cpi::DelegateConfig;
use pyth_solana_receiver_sdk::price_update::{PriceFeedMessage, PriceUpdateV2, VerificationLevel};

declare_id!("PriCems5tHihc6UDXDjzjeawomAwBduWMGAi8ZUjppd");

#[ephemeral]
#[program]
pub mod ephemeral_oracle {
    use super::*;

    pub fn initialize_price_feed(
        ctx: Context<InitializePriceFeed>,
        _provider: String,
        _symbol: String,
        feed_id: [u8; 32],
        exponent: i32,
    ) -> Result<()> {
        let price_feed = &mut ctx.accounts.price_feed;
        price_feed.posted_slot = 0;
        price_feed.verification_level = VerificationLevel::Full;
        price_feed.price_message = PriceFeedMessage {
            feed_id,
            ema_conf: 0,
            ema_price: 0,
            price: 0,
            conf: 0,
            exponent,
            prev_publish_time: Clock::get()?.unix_timestamp,
            publish_time: Clock::get()?.unix_timestamp,
        };
        Ok(())
    }

    pub fn update_price_feed(
        ctx: Context<UpdatePriceFeed>,
        _provider: String,
        update_data: UpdateData,
    ) -> Result<()> {
        let price_feed = &mut ctx.accounts.price_feed;

        // TODO: verify the message signature

        let price = update_data.temporal_numeric_value.quantized_value  as i64;

        price_feed.posted_slot = Clock::get()?.slot;
        price_feed.price_message = PriceFeedMessage {
            feed_id: price_feed.price_message.feed_id,
            ema_conf: price_feed.price_message.ema_conf,
            ema_price: price_feed.price_message.ema_price,
            conf: price_feed.price_message.conf,
            exponent: price_feed.price_message.exponent,
            prev_publish_time: price_feed.price_message.publish_time,
            price,
            publish_time: Clock::get()?.unix_timestamp,
        };
        price_feed.verification_level = VerificationLevel::Full;

        msg!("The price update is: {}", price_feed.price_message.price);
        msg!("The exponent is: {}", price_feed.price_message.exponent);

        //price_feed.try_serialize(&mut *ctx.accounts.price_feed.data.borrow_mut())?;
        Ok(())
    }

    pub fn delegate_price_feed(
        ctx: Context<DelegatePriceFeed>,
        provider: String,
        symbol: String,
    ) -> Result<()> {
        ctx.accounts.delegate_price_feed(
            &ctx.accounts.payer,
            &[
                InitializePriceFeed::seed(),
                provider.as_bytes(),
                symbol.as_bytes(),
            ],
            DelegateConfig::default(),
        )?;
        Ok(())
    }

    pub fn undelegate_price_feed(ctx: Context<UndelegatePriceFeed>, _provider: String, _symbol: String) -> Result<()> {
        commit_and_undelegate_accounts(
            &ctx.accounts.payer,
            vec![&ctx.accounts.price_feed.to_account_info()],
            &ctx.accounts.magic_context,
            &ctx.accounts.magic_program,
        )?;
        Ok(())
    }

    pub fn close_price_feed(ctx: Context<ClosePriceFeed>, _provider: String, _symbol: String) -> Result<()> {
        close_pda(&ctx.accounts.price_feed, &ctx.accounts.payer.to_account_info())?;
        Ok(())
    }

    pub fn sample(ctx: Context<Sample>) -> Result<()> {
        // Deserialize the price feed
        let price_update = PriceUpdateV2::try_deserialize_unchecked(
            &mut (*ctx.accounts.price_update.data.borrow()).as_ref(),
        )
        .map_err(Into::<Error>::into)?;

        // get_price_no_older_than will fail if the price update is more than 30 seconds old
        let maximum_age: u64 = 60;

        // Get the price feed id
        let feed_id: [u8; 32] = ctx.accounts.price_update.key().to_bytes();

        let price = price_update.get_price_no_older_than(&Clock::get()?, maximum_age, &feed_id)?;

        // Sample output:
        // The price is (7160106530699 ± 5129162301) * 10^-8
        msg!(
            "The price is ({} ± {}) * 10^-{}",
            price.price,
            price.conf,
            price.exponent
        );
        msg!(
            "The price is: {}",
            price.price as f64 * 10_f64.powi(-price.exponent)
        );
        msg!("Slot: {}", price_update.posted_slot);
        msg!("Message: {:?}", price_update.price_message);

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(provider: String, symbol: String, feed_id: [u8; 32], exponent: i32)]
pub struct InitializePriceFeed<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: the correct price feed
    #[account(init, payer = payer, space = PriceUpdateV2::LEN, seeds = [InitializePriceFeed::seed(), provider.as_bytes(), symbol.as_bytes()], bump)]
    pub price_feed: Account<'info, PriceUpdateV3>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(provider: String, update_data: UpdateData)]
pub struct UpdatePriceFeed<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: the correct price feed
    #[account(mut, seeds = [InitializePriceFeed::seed(), provider.as_bytes(), update_data.symbol.as_bytes()], bump)]
    pub price_feed: Account<'info, PriceUpdateV3>,
}

#[delegate]
#[derive(Accounts)]
#[instruction(provider: String, symbol: String)]
pub struct DelegatePriceFeed<'info> {
    pub payer: Signer<'info>,
    /// CHECK The pda to delegate
    #[account(mut, del, seeds = [InitializePriceFeed::seed(), provider.as_bytes(), symbol.as_bytes()], bump)]
    pub price_feed: AccountInfo<'info>,
}

#[commit]
#[derive(Accounts)]
#[instruction(provider: String, symbol: String)]
pub struct UndelegatePriceFeed<'info> {
    pub payer: Signer<'info>,
    /// CHECK The pda to undelegate
    #[account(mut, seeds = [InitializePriceFeed::seed(), provider.as_bytes(), symbol.as_bytes()], bump)]
    pub price_feed: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(provider: String, symbol: String)]
pub struct ClosePriceFeed<'info> {
    pub payer: Signer<'info>,
    /// CHECK The pda to close
    #[account(mut, seeds = [InitializePriceFeed::seed(), provider.as_bytes(), symbol.as_bytes()], bump)]
    pub price_feed: AccountInfo<'info>}

#[derive(Accounts)]
pub struct Sample<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: the correct price feed
    pub price_update: AccountInfo<'info>,
}

impl InitializePriceFeed<'_> {
    pub fn seed() -> &'static [u8] {
        b"price_feed"
    }
}

#[account]
#[derive(BorshSchema)]
pub struct PriceUpdateV3 {
    pub write_authority: Pubkey,
    pub verification_level: VerificationLevel,
    pub price_message: PriceFeedMessage,
    pub posted_slot: u64,
}
