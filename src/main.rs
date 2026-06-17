use std::{env, path::PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solana_keypair::{Keypair, read_keypair_file};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

const DEFAULT_JUPITER_BASE_URL: &str = "https://api.jup.ag/swap/v2";
const DEFAULT_RAYDIUM_BASE_URL: &str = "https://transaction-v1.raydium.io";
const DEFAULT_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const DEFAULT_INPUT_MINT: &str = "So11111111111111111111111111111111111111112";
const DEFAULT_OUTPUT_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const DEFAULT_AMOUNT_RAW: u64 = 100_000_000;
const DEFAULT_SLIPPAGE_BPS: u16 = 50;
const DEFAULT_TX_VERSION: &str = "V0";
const DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS: &str = "50000";
const NATIVE_SOL_MINT: &str = "So11111111111111111111111111111111111111112";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dex {
    Jupiter,
    Raydium,
}

#[derive(Debug)]
struct Config {
    dex: Dex,
    jupiter_base_url: String,
    jupiter_api_key: Option<String>,
    raydium_base_url: String,
    rpc_url: String,
    keypair: Keypair,
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_raw: u64,
    slippage_bps: u16,
    tx_version: String,
    compute_unit_price_micro_lamports: String,
    execute: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JupiterOrderResponse {
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
struct JupiterExecuteRequest {
    signed_transaction: String,
    request_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JupiterExecuteResponse {
    status: String,
    signature: Option<String>,
    code: Option<i64>,
    input_amount_result: Option<String>,
    output_amount_result: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RaydiumQuoteResponse {
    id: Option<String>,
    success: bool,
    version: Option<String>,
    data: Option<Value>,
    msg: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RaydiumTransactionResponse {
    id: Option<String>,
    success: bool,
    version: Option<String>,
    data: Option<Vec<RaydiumTransactionEntry>>,
    msg: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RaydiumTransactionEntry {
    transaction: String,
}

#[derive(Debug, Deserialize)]
struct RpcSendResponse {
    result: Option<String>,
    error: Option<Value>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let config = Config::from_env_and_args()?;
    let wallet = config.keypair.pubkey();
    let client = Client::new();

    println!("Wallet: {wallet}");
    println!(
        "Swap: {} raw units from {} -> {} via {:?}",
        config.amount_raw, config.input_mint, config.output_mint, config.dex
    );

    match config.dex {
        Dex::Jupiter => run_jupiter(&client, &config, &wallet).await,
        Dex::Raydium => run_raydium(&client, &config, &wallet).await,
    }
}

impl Config {
    fn from_env_and_args() -> Result<Self> {
        let dex = parse_dex(&env::var("DEX").unwrap_or_else(|_| "jupiter".to_string()))?;
        let execute = env::args().any(|arg| arg == "--execute")
            || env_bool("EXECUTE").context("EXECUTE must be true/false, yes/no, or 1/0")?;
        let jupiter_base_url =
            env::var("JUPITER_BASE_URL").unwrap_or_else(|_| DEFAULT_JUPITER_BASE_URL.to_string());
        let jupiter_api_key = env_optional("JUPITER_API_KEY")?;
        let raydium_base_url =
            env::var("RAYDIUM_BASE_URL").unwrap_or_else(|_| DEFAULT_RAYDIUM_BASE_URL.to_string());
        let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());
        let keypair = load_keypair()?;
        let input_mint = env_pubkey("INPUT_MINT", DEFAULT_INPUT_MINT)?;
        let output_mint = env_pubkey("OUTPUT_MINT", DEFAULT_OUTPUT_MINT)?;
        let amount_raw = env_u64("AMOUNT_RAW", DEFAULT_AMOUNT_RAW)?;
        let slippage_bps = env_u16("SLIPPAGE_BPS", DEFAULT_SLIPPAGE_BPS)?;
        let tx_version = env::var("TX_VERSION").unwrap_or_else(|_| DEFAULT_TX_VERSION.to_string());
        let compute_unit_price_micro_lamports = env::var("COMPUTE_UNIT_PRICE_MICRO_LAMPORTS")
            .unwrap_or_else(|_| DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS.to_string());

        if dex == Dex::Jupiter && jupiter_api_key.is_none() {
            bail!("JUPITER_API_KEY is required when DEX=jupiter");
        }

        Ok(Self {
            dex,
            jupiter_base_url,
            jupiter_api_key,
            raydium_base_url,
            rpc_url,
            keypair,
            input_mint,
            output_mint,
            amount_raw,
            slippage_bps,
            tx_version,
            compute_unit_price_micro_lamports,
            execute,
        })
    }
}

async fn run_jupiter(client: &Client, config: &Config, wallet: &Pubkey) -> Result<()> {
    let order = get_jupiter_order(client, config, wallet).await?;
    print_jupiter_order_summary(&order)?;

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

    let result = execute_jupiter_swap(client, config, &order.request_id, &signed_tx).await?;
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

async fn run_raydium(client: &Client, config: &Config, wallet: &Pubkey) -> Result<()> {
    let quote = get_raydium_quote(client, config).await?;
    ensure_raydium_success("Raydium quote", quote.success, &quote.msg, &quote.error)?;
    print_raydium_quote_summary(&quote)?;

    let built = build_raydium_transactions(client, config, wallet, &quote).await?;
    ensure_raydium_success(
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

async fn get_jupiter_order(
    client: &Client,
    config: &Config,
    wallet: &Pubkey,
) -> Result<JupiterOrderResponse> {
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
        .await
        .context("Failed to request Jupiter order")?;

    parse_json_response(response, "Jupiter API").await
}

async fn execute_jupiter_swap(
    client: &Client,
    config: &Config,
    request_id: &str,
    signed_transaction: &str,
) -> Result<JupiterExecuteResponse> {
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
        .json(&JupiterExecuteRequest {
            signed_transaction: signed_transaction.to_string(),
            request_id: request_id.to_string(),
        })
        .send()
        .await
        .context("Failed to request Jupiter execute")?;

    parse_json_response(response, "Jupiter API").await
}

async fn get_raydium_quote(client: &Client, config: &Config) -> Result<RaydiumQuoteResponse> {
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
        .await
        .context("Failed to request Raydium quote")?;

    parse_json_response(response, "Raydium API").await
}

async fn build_raydium_transactions(
    client: &Client,
    config: &Config,
    wallet: &Pubkey,
    quote: &RaydiumQuoteResponse,
) -> Result<RaydiumTransactionResponse> {
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
        .await
        .context("Failed to build Raydium transaction")?;

    parse_json_response(response, "Raydium API").await
}

async fn send_transaction(
    client: &Client,
    rpc_url: &str,
    signed_transaction: &str,
) -> Result<String> {
    let response = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                signed_transaction,
                {
                    "encoding": "base64",
                    "skipPreflight": false,
                    "preflightCommitment": "confirmed"
                }
            ]
        }))
        .send()
        .await
        .context("Failed to send RPC request")?;

    let rpc: RpcSendResponse = parse_json_response(response, "Solana RPC").await?;
    if let Some(error) = rpc.error {
        bail!("Solana RPC sendTransaction failed: {error}");
    }
    rpc.result
        .ok_or_else(|| anyhow!("Solana RPC did not return a transaction signature"))
}

async fn parse_json_response<T>(response: reqwest::Response, label: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    let body = response
        .text()
        .await
        .context("Failed to read response body")?;

    if status != StatusCode::OK {
        bail!("{label} returned {status}: {body}");
    }

    serde_json::from_str(&body).with_context(|| format!("Failed to parse {label} response: {body}"))
}

fn sign_transaction_base64(encoded_tx: &str, keypair: &Keypair, label: &str) -> Result<String> {
    let tx_bytes = BASE64
        .decode(encoded_tx)
        .with_context(|| format!("{label} transaction is not valid base64"))?;
    let mut tx: VersionedTransaction = bincode::deserialize(&tx_bytes)
        .with_context(|| format!("Failed to deserialize {label} transaction"))?;

    let required_signatures = tx.message.header().num_required_signatures as usize;
    let signer_index = tx
        .message
        .static_account_keys()
        .iter()
        .take(required_signatures)
        .position(|pubkey| pubkey == &keypair.pubkey())
        .ok_or_else(|| anyhow!("Wallet {} is not a required signer", keypair.pubkey()))?;

    let message_bytes = tx.message.serialize();
    tx.signatures[signer_index] = keypair.sign_message(&message_bytes);

    let signed_tx_bytes = bincode::serialize(&tx)
        .with_context(|| format!("Failed to serialize {label} transaction"))?;
    Ok(BASE64.encode(signed_tx_bytes))
}

fn load_keypair() -> Result<Keypair> {
    if let Some(private_key) = env_optional("BS58_PRIVATE_KEY")? {
        let bytes = bs58::decode(private_key.trim())
            .into_vec()
            .context("BS58_PRIVATE_KEY is not valid base58")?;

        return match bytes.len() {
            64 => Keypair::try_from(bytes.as_slice())
                .context("BS58_PRIVATE_KEY is not a valid 64-byte Solana keypair"),
            32 => {
                let seed: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| anyhow!("BS58_PRIVATE_KEY seed must be 32 bytes"))?;
                Ok(Keypair::new_from_array(seed))
            }
            len => bail!(
                "BS58_PRIVATE_KEY decoded to {len} bytes; expected 32-byte seed or 64-byte keypair"
            ),
        };
    }

    let path = env::var("SOLANA_KEYPAIR_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config/solana/id.json")
        });

    read_keypair_file(&path)
        .map_err(|err| anyhow!("Failed to read keypair {}: {err}", path.display()))
}

fn parse_dex(value: &str) -> Result<Dex> {
    match value.trim().to_ascii_lowercase().as_str() {
        "jupiter" | "jup" => Ok(Dex::Jupiter),
        "raydium" | "ray" => Ok(Dex::Raydium),
        _ => bail!("DEX must be jupiter or raydium"),
    }
}

fn env_pubkey(name: &str, default: &str) -> Result<Pubkey> {
    let value = env::var(name).unwrap_or_else(|_| default.to_string());
    value
        .parse()
        .with_context(|| format!("{name} is not a valid Solana pubkey"))
}

fn env_u64(name: &str, default: u64) -> Result<u64> {
    env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse()
        .with_context(|| format!("{name} must be a positive integer"))
}

fn env_u16(name: &str, default: u16) -> Result<u16> {
    env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse()
        .with_context(|| format!("{name} must be an integer between 0 and 65535"))
}

fn env_optional(name: &str) -> Result<Option<String>> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(err).with_context(|| format!("Failed to read {name}")),
    }
}

fn env_bool(name: &str) -> Result<bool> {
    match env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "y" => Ok(true),
            "false" | "0" | "no" | "n" => Ok(false),
            _ => bail!("{name} has invalid boolean value"),
        },
        Err(env::VarError::NotPresent) => Ok(false),
        Err(err) => Err(err).with_context(|| format!("Failed to read {name}")),
    }
}

fn ensure_raydium_success(
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

fn print_jupiter_order_summary(order: &JupiterOrderResponse) -> Result<()> {
    println!("Router: {}", order.router.as_deref().unwrap_or("unknown"));
    println!("Mode: {}", order.mode.as_deref().unwrap_or("unknown"));
    println!("Estimated output raw amount: {}", order.out_amount);
    if let Some(fee_bps) = order.fee_bps {
        println!("Jupiter fee: {fee_bps} bps");
    }
    if let Some(fee_mint) = &order.fee_mint {
        println!("Fee mint: {fee_mint}");
    }

    Ok(())
}

fn print_raydium_quote_summary(quote: &RaydiumQuoteResponse) -> Result<()> {
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
