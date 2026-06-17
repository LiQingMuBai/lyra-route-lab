mod config;
mod http;
mod jupiter;
mod raydium;
mod tokens;
mod tx;

use anyhow::Result;
use config::{Config, Dex};
use reqwest::Client;
use solana_signer::Signer;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let config = Config::from_env_and_args()?;
    let wallet = config.keypair.pubkey();
    let client = Client::new();

    println!("Wallet: {wallet}");
    println!(
        "Swap: {} raw units from {} ({}) -> {} ({}) via {:?}",
        config.amount_raw,
        config.input_symbol,
        config.input_mint,
        config.output_symbol,
        config.output_mint,
        config.dex
    );

    match config.dex {
        Dex::Jupiter => jupiter::run(&client, &config, &wallet).await,
        Dex::Raydium => raydium::run(&client, &config, &wallet).await,
    }
}
