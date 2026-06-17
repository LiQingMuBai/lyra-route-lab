use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use crate::http::parse_json_response;

#[derive(Debug, Deserialize)]
struct RpcSendResponse {
    result: Option<String>,
    error: Option<Value>,
}

pub fn sign_transaction_base64(encoded_tx: &str, keypair: &Keypair, label: &str) -> Result<String> {
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

pub async fn send_transaction(
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
