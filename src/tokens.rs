use std::env;

use anyhow::{Context, Result, anyhow, bail};
use solana_pubkey::Pubkey;

use crate::config::env_optional;

const DEFAULT_AMOUNT_RAW: u64 = 100_000_000;

#[derive(Debug, Clone, Copy)]
struct TokenInfo {
    symbol: &'static str,
    mint: &'static str,
    decimals: u8,
}

const TOKENS: &[TokenInfo] = &[
    TokenInfo {
        symbol: "SOL",
        mint: "So11111111111111111111111111111111111111112",
        decimals: 9,
    },
    TokenInfo {
        symbol: "USDC",
        mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        decimals: 6,
    },
    TokenInfo {
        symbol: "USDT",
        mint: "Es9vMFrzaCERmJfrF4H2FYD4x6Vp7t7GPuE3kBX6QKc",
        decimals: 6,
    },
    TokenInfo {
        symbol: "RAY",
        mint: "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R",
        decimals: 6,
    },
    TokenInfo {
        symbol: "JUP",
        mint: "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
        decimals: 6,
    },
];

#[derive(Debug)]
pub struct ResolvedToken {
    pub symbol: String,
    pub mint: Pubkey,
    pub decimals: u8,
}

pub fn resolve_token(
    symbol_env: &str,
    mint_env: &str,
    decimals_env: &str,
    default_symbol: &str,
) -> Result<ResolvedToken> {
    if let Some(mint) = env_optional(mint_env)? {
        let symbol = env::var(symbol_env).unwrap_or_else(|_| "CUSTOM".to_string());
        let token = token_by_symbol(&symbol);
        let decimals = match env_optional(decimals_env)? {
            Some(value) => value
                .parse()
                .with_context(|| format!("{decimals_env} must be an integer between 0 and 255"))?,
            None => token
                .map(|token| token.decimals)
                .ok_or_else(|| anyhow!("{mint_env} is custom; set {decimals_env} explicitly"))?,
        };

        return Ok(ResolvedToken {
            symbol,
            mint: mint
                .parse()
                .with_context(|| format!("{mint_env} is not a valid Solana pubkey"))?,
            decimals,
        });
    }

    let symbol = env::var(symbol_env).unwrap_or_else(|_| default_symbol.to_string());
    let token = token_by_symbol(&symbol).ok_or_else(|| {
        anyhow!(
            "Unsupported {symbol_env}={symbol}. Use {mint_env} plus decimals for custom tokens."
        )
    })?;

    Ok(ResolvedToken {
        symbol: token.symbol.to_string(),
        mint: token
            .mint
            .parse()
            .with_context(|| format!("Built-in mint for {} is invalid", token.symbol))?,
        decimals: token.decimals,
    })
}

pub fn resolve_amount_raw(input_token: &ResolvedToken) -> Result<u64> {
    if let Some(amount_raw) = env_optional("AMOUNT_RAW")? {
        return amount_raw
            .parse()
            .context("AMOUNT_RAW must be a positive integer");
    }

    if let Some(amount) = env_optional("AMOUNT")? {
        return decimal_amount_to_raw(&amount, input_token.decimals).with_context(|| {
            format!(
                "Failed to convert AMOUNT using {} decimals",
                input_token.decimals
            )
        });
    }

    Ok(DEFAULT_AMOUNT_RAW)
}

fn token_by_symbol(symbol: &str) -> Option<TokenInfo> {
    let symbol = symbol.trim().to_ascii_uppercase();
    TOKENS
        .iter()
        .copied()
        .find(|token| token.symbol == symbol.as_str())
}

fn decimal_amount_to_raw(amount: &str, decimals: u8) -> Result<u64> {
    let trimmed = amount.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        bail!("AMOUNT must be a positive decimal");
    }

    let parts = trimmed.split('.').collect::<Vec<_>>();
    if parts.len() > 2 {
        bail!("AMOUNT has too many decimal points");
    }

    let whole = parts[0];
    let fraction = parts.get(1).copied().unwrap_or("");
    if whole.is_empty() && fraction.is_empty() {
        bail!("AMOUNT must include digits");
    }
    if !whole.chars().all(|ch| ch.is_ascii_digit()) {
        bail!("AMOUNT whole part must be numeric");
    }
    if !fraction.chars().all(|ch| ch.is_ascii_digit()) {
        bail!("AMOUNT fractional part must be numeric");
    }
    if fraction.len() > decimals as usize {
        bail!("AMOUNT has more fractional digits than token decimals ({decimals})");
    }

    let mut raw = whole
        .parse::<u64>()
        .context("AMOUNT whole part is too large")?;
    raw = raw
        .checked_mul(10_u64.pow(decimals as u32))
        .ok_or_else(|| anyhow!("AMOUNT is too large"))?;

    let mut padded_fraction = fraction.to_string();
    padded_fraction.extend(std::iter::repeat_n(
        '0',
        decimals as usize - padded_fraction.len(),
    ));
    let fraction_raw = if padded_fraction.is_empty() {
        0
    } else {
        padded_fraction
            .parse::<u64>()
            .context("AMOUNT fractional part is too large")?
    };

    raw.checked_add(fraction_raw)
        .ok_or_else(|| anyhow!("AMOUNT is too large"))
}
