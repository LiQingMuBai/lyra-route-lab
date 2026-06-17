use anyhow::{Context, Result, anyhow, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solana_pubkey::Pubkey;

use crate::{
    config::Config,
    http::parse_json_response,
    tx::{send_transaction, sign_transaction_base64},
};

const NATIVE_SOL_MINT: &str = "So11111111111111111111111111111111111111112";

#[derive(Debug, Deserialize, Serialize)]
struct QuoteResponse {
    id: Option<String>,
    success: bool,
    version: Option<String>,
    data: Option<Value>,
    msg: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransactionResponse {
    id: Option<String>,
    success: bool,
    version: Option<String>,
    data: Option<Vec<TransactionEntry>>,
    msg: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransactionEntry {
    transaction: String,
}

pub async fn run(client: &Client, config: &Config, wallet: &Pubkey) -> Result<()> {
    let quote = get_quote(client, config).await?;
    ensure_success("Raydium quote", quote.success, &quote.msg, &quote.error)?;
    print_quote_summary(&quote)?;

    let built = build_transactions(client, config, wallet, &quote).await?;
    ensure_success(
        "Raydium transaction build",
        built.success,
        &built.msg,
        &built.error,
    )?;
    println!(
        "Raydium build id: {}, version: {}",
        built.id.as_deref().unwrap_or("unknown"),
        built.version.as_deref().unwrap_or("unknown")
    );

    let entries = built
        .data
        .as_ref()
        .filter(|data| !data.is_empty())
        .ok_or_else(|| anyhow!("Raydium did not return any transaction data"))?;

    let signed_transactions = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            sign_transaction_base64(
                &entry.transaction,
                &config.keypair,
                &format!("Raydium transaction #{}", index + 1),
            )
        })
        .collect::<Result<Vec<_>>>()?;

    println!(
        "Raydium returned {} transaction(s); all were signed locally.",
        signed_transactions.len()
    );

    if !config.execute {
        println!();
        println!(
            "Dry run complete. Raydium transactions were built and signed, but not broadcast."
        );
        println!("Set EXECUTE=true in .env or run with --execute to send them through RPC_URL.");
        return Ok(());
    }

    for (index, signed_tx) in signed_transactions.iter().enumerate() {
        let signature = send_transaction(client, &config.rpc_url, signed_tx)
            .await
            .with_context(|| format!("Failed to send Raydium transaction #{}", index + 1))?;
        println!(
            "Sent Raydium transaction #{}: https://solscan.io/tx/{}",
            index + 1,
            signature
        );
    }

    Ok(())
}

async fn get_quote(client: &Client, config: &Config) -> Result<QuoteResponse> {
    let response = client
        .get(format!(
            "{}/compute/swap-base-in",
            config.raydium_base_url.trim_end_matches('/')
        ))
        .query(&[
            ("inputMint", config.input_mint.to_string()),
            ("outputMint", config.output_mint.to_string()),
            ("amount", config.amount_raw.to_string()),
            ("slippageBps", config.slippage_bps.to_string()),
            ("txVersion", config.tx_version.clone()),
        ])
        .send()
        .await?;

    parse_json_response(response, "Raydium API").await
}

async fn build_transactions(
    client: &Client,
    config: &Config,
    wallet: &Pubkey,
    quote: &QuoteResponse,
) -> Result<TransactionResponse> {
    let input_is_sol = config.input_mint.to_string() == NATIVE_SOL_MINT;
    let output_is_sol = config.output_mint.to_string() == NATIVE_SOL_MINT;
    let response = client
        .post(format!(
            "{}/transaction/swap-base-in",
            config.raydium_base_url.trim_end_matches('/')
        ))
        .json(&json!({
            "computeUnitPriceMicroLamports": config.compute_unit_price_micro_lamports,
            "swapResponse": quote,
            "txVersion": config.tx_version,
            "wallet": wallet.to_string(),
            "wrapSol": input_is_sol,
            "unwrapSol": output_is_sol,
        }))
        .send()
        .await?;

    parse_json_response(response, "Raydium API").await
}

fn ensure_success(
    label: &str,
    success: bool,
    msg: &Option<String>,
    error: &Option<String>,
) -> Result<()> {
    if success {
        return Ok(());
    }

    bail!("{label} failed: msg={msg:?}, error={error:?}")
}

fn print_quote_summary(quote: &QuoteResponse) -> Result<()> {
    let data = quote
        .data
        .as_ref()
        .ok_or_else(|| anyhow!("Raydium quote response did not include data"))?;

    println!(
        "Raydium quote id: {}",
        quote.id.as_deref().unwrap_or("unknown")
    );
    println!(
        "Raydium API version: {}",
        quote.version.as_deref().unwrap_or("unknown")
    );

    if let Some(output_amount) = data.get("outputAmount").and_then(Value::as_str) {
        println!("Estimated output raw amount: {output_amount}");
    }
    if let Some(threshold) = data.get("otherAmountThreshold").and_then(Value::as_str) {
        println!("Minimum output after slippage: {threshold}");
    }
    if let Some(price_impact) = data.get("priceImpactPct") {
        println!("Price impact pct: {price_impact}");
    }

    Ok(())
}
