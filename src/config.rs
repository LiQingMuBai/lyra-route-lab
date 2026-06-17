use std::{env, path::PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use solana_keypair::{Keypair, read_keypair_file};
use solana_pubkey::Pubkey;

use crate::tokens::{resolve_amount_raw, resolve_token};

const DEFAULT_JUPITER_BASE_URL: &str = "https://api.jup.ag/swap/v2";
const DEFAULT_RAYDIUM_BASE_URL: &str = "https://transaction-v1.raydium.io";
const DEFAULT_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const DEFAULT_SLIPPAGE_BPS: u16 = 50;
const DEFAULT_TX_VERSION: &str = "V0";
const DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS: &str = "50000";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dex {
    Jupiter,
    Raydium,
}

#[derive(Debug)]
pub struct Config {
    pub dex: Dex,
    pub jupiter_base_url: String,
    pub jupiter_api_key: Option<String>,
    pub raydium_base_url: String,
    pub rpc_url: String,
    pub keypair: Keypair,
    pub input_symbol: String,
    pub output_symbol: String,
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub amount_raw: u64,
    pub slippage_bps: u16,
    pub tx_version: String,
    pub compute_unit_price_micro_lamports: String,
    pub execute: bool,
}

impl Config {
    pub fn from_env_and_args() -> Result<Self> {
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
        let input_token = resolve_token("INPUT_TOKEN", "INPUT_MINT", "INPUT_DECIMALS", "SOL")?;
        let output_token = resolve_token("OUTPUT_TOKEN", "OUTPUT_MINT", "OUTPUT_DECIMALS", "USDC")?;
        let amount_raw = resolve_amount_raw(&input_token)?;
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
            input_symbol: input_token.symbol,
            output_symbol: output_token.symbol,
            input_mint: input_token.mint,
            output_mint: output_token.mint,
            amount_raw,
            slippage_bps,
            tx_version,
            compute_unit_price_micro_lamports,
            execute,
        })
    }
}

pub fn env_optional(name: &str) -> Result<Option<String>> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(err).with_context(|| format!("Failed to read {name}")),
    }
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

fn env_u16(name: &str, default: u16) -> Result<u16> {
    env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse()
        .with_context(|| format!("{name} must be an integer between 0 and 65535"))
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
