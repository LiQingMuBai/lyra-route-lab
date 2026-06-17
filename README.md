# Lyra Route Lab

Rust CLI for researching and executing Solana DEX routes through Jupiter or Raydium. The current tool focuses on safe single-route execution, local transaction signing, and configurable routing parameters.

## Configure

All parameters are read from `.env`:

```dotenv
DEX=raydium

JUPITER_API_KEY=your_jupiter_api_key
JUPITER_BASE_URL=https://api.jup.ag/swap/v2
RAYDIUM_BASE_URL=https://transaction-v1.raydium.io
RPC_URL=https://api.mainnet-beta.solana.com

INPUT_MINT=So11111111111111111111111111111111111111112
OUTPUT_MINT=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v
AMOUNT_RAW=100000000
SLIPPAGE_BPS=50
TX_VERSION=V0
COMPUTE_UNIT_PRICE_MICRO_LAMPORTS=50000

BS58_PRIVATE_KEY=your_base58_encoded_private_key
# SOLANA_KEYPAIR_PATH=/Users/masion/.config/solana/id.json

EXECUTE=false
```

`DEX=raydium` uses Raydium Trade API. `DEX=jupiter` uses Jupiter Swap API.

`AMOUNT_RAW=100000000` equals 0.1 SOL because SOL has 9 decimals. For USDC, 1 USDC is `1000000` raw units because Solana USDC has 6 decimals.

Use either `BS58_PRIVATE_KEY` or `SOLANA_KEYPAIR_PATH`. If both are set, `BS58_PRIVATE_KEY` takes priority. `BS58_PRIVATE_KEY` can be either a 64-byte Solana keypair or a 32-byte seed encoded as base58.

## Run

Build and sign the transaction without broadcasting:

```bash
cargo run
```

Submit the signed transaction:

```dotenv
EXECUTE=true
```

Then run:

```bash
cargo run
```

You can also keep `EXECUTE=false` and submit once with:

```bash
cargo run -- --execute
```
