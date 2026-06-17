use anyhow::{Result, anyhow, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

use crate::{config::Config, http::parse_json_response, tx::sign_transaction_base64};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderResponse {
    transaction: Option<String>,
    request_id: String,
    out_amount: String,
    router: Option<String>,
    mode: Option<String>,
    fee_bps: Option<u16>,
    fee_mint: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteRequest {
    signed_transaction: String,
    request_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteResponse {
    status: String,
    signature: Option<String>,
    code: Option<i64>,
    input_amount_result: Option<String>,
    output_amount_result: Option<String>,
    error: Option<String>,
}

pub async fn run(client: &Client, config: &Config, wallet: &Pubkey) -> Result<()> {
    let order = get_order(client, config, wallet).await?;
    print_order_summary(&order);

    let unsigned_tx = order.transaction.as_deref().ok_or_else(|| {
        anyhow!("Jupiter did not return a transaction. Check wallet/token balance and API key.")
    })?;
    let signed_tx = sign_transaction_base64(unsigned_tx, &config.keypair, "Jupiter")?;

    if !config.execute {
        println!();
        println!(
            "Dry run complete. The Jupiter transaction was built and signed, but not broadcast."
        );
        println!(
            "Set EXECUTE=true in .env or run with --execute to submit it to Jupiter /execute."
        );
        return Ok(());
    }

    let result = execute_swap(client, config, &order.request_id, &signed_tx).await?;
    println!();
    println!(
        "Jupiter execute response: {}",
        serde_json::to_string_pretty(&result)?
    );

    if result.status == "Success" {
        if let Some(signature) = result.signature {
            println!("Solscan: https://solscan.io/tx/{signature}");
        }
    } else {
        bail!(
            "Jupiter swap failed: status={}, code={:?}, error={:?}",
            result.status,
            result.code,
            result.error
        );
    }

    Ok(())
}

async fn get_order(client: &Client, config: &Config, wallet: &Pubkey) -> Result<OrderResponse> {
    let response = client
        .get(format!(
            "{}/order",
            config.jupiter_base_url.trim_end_matches('/')
        ))
        .header(
            "x-api-key",
            config
                .jupiter_api_key
                .as_deref()
                .ok_or_else(|| anyhow!("Missing JUPITER_API_KEY"))?,
        )
        .query(&[
            ("inputMint", config.input_mint.to_string()),
            ("outputMint", config.output_mint.to_string()),
            ("amount", config.amount_raw.to_string()),
            ("taker", wallet.to_string()),
        ])
        .send()
        .await?;

    parse_json_response(response, "Jupiter API").await
}

async fn execute_swap(
    client: &Client,
    config: &Config,
    request_id: &str,
    signed_transaction: &str,
) -> Result<ExecuteResponse> {
    let response = client
        .post(format!(
            "{}/execute",
            config.jupiter_base_url.trim_end_matches('/')
        ))
        .header(
            "x-api-key",
            config
                .jupiter_api_key
                .as_deref()
                .ok_or_else(|| anyhow!("Missing JUPITER_API_KEY"))?,
        )
        .json(&ExecuteRequest {
            signed_transaction: signed_transaction.to_string(),
            request_id: request_id.to_string(),
        })
        .send()
        .await?;

    parse_json_response(response, "Jupiter API").await
}

fn print_order_summary(order: &OrderResponse) {
    println!("Router: {}", order.router.as_deref().unwrap_or("unknown"));
    println!("Mode: {}", order.mode.as_deref().unwrap_or("unknown"));
    println!("Estimated output raw amount: {}", order.out_amount);
    if let Some(fee_bps) = order.fee_bps {
        println!("Jupiter fee: {fee_bps} bps");
    }
    if let Some(fee_mint) = &order.fee_mint {
        println!("Fee mint: {fee_mint}");
    }
}
