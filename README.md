# Lyra Route Lab

Rust CLI for researching and executing Solana DEX routes through Jupiter or Raydium. It is designed as a small, inspectable base for route experiments: configurable tokens, local transaction signing, dry-run first, and separate DEX modules.

![Rust](https://img.shields.io/badge/Rust-2024-b7410e)
![Solana](https://img.shields.io/badge/Solana-mainnet-14f195)
![DEX](https://img.shields.io/badge/DEX-Jupiter%20%7C%20Raydium-4f46e5)

## Overview

```mermaid
flowchart LR
    ENV[".env config"] --> CFG["config.rs"]
    CFG --> TOK["tokens.rs"]
    CFG --> MAIN["main.rs"]
    MAIN --> JUP["jupiter.rs"]
    MAIN --> RAY["raydium.rs"]
    JUP --> SIGN["tx.rs local signing"]
    RAY --> SIGN
    RAY --> RPC["Solana RPC"]
    JUP --> JAPI["Jupiter Swap API"]
    RAY --> RAPI["Raydium Trade API"]
```

## Execution Flow

```mermaid
sequenceDiagram
    participant User
    participant CLI as Lyra Route Lab
    participant DEX as Jupiter/Raydium
    participant Wallet as Local Keypair
    participant RPC as Solana RPC

    User->>CLI: cargo run
    CLI->>CLI: Load .env
    CLI->>DEX: Quote/build swap transaction
    DEX-->>CLI: Base64 transaction
    CLI->>Wallet: Sign locally
    alt EXECUTE=false
        CLI-->>User: Dry-run result, no broadcast
    else EXECUTE=true
        CLI->>RPC: Send signed transaction
        RPC-->>CLI: Signature
        CLI-->>User: Solscan link
    end
```

## Modules

| File | Role |
| --- | --- |
| `src/main.rs` | Entry point. Loads config and dispatches to the selected DEX. |
| `src/config.rs` | Reads `.env`, loads wallet, validates DEX selection. |
| `src/jupiter.rs` | Jupiter `/order` and `/execute` flow. |
| `src/raydium.rs` | Raydium quote, transaction build, and RPC send flow. |
| `src/tokens.rs` | Token symbol registry and decimal amount conversion. |
| `src/tx.rs` | Base64 transaction decoding, local signing, and RPC send. |
| `src/http.rs` | Shared HTTP JSON response parsing. |

## Configure

Copy `.env.example` to `.env`, then edit the values:

```dotenv
DEX=raydium

JUPITER_API_KEY=your_jupiter_api_key
JUPITER_BASE_URL=https://api.jup.ag/swap/v2
RAYDIUM_BASE_URL=https://transaction-v1.raydium.io
RPC_URL=https://api.mainnet-beta.solana.com

INPUT_TOKEN=SOL
OUTPUT_TOKEN=USDC
AMOUNT=0.1
# AMOUNT_RAW=100000000

# For tokens not built into the local registry, use mint + decimals:
# INPUT_MINT=So11111111111111111111111111111111111111112
# INPUT_DECIMALS=9
# OUTPUT_MINT=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v
# OUTPUT_DECIMALS=6

SLIPPAGE_BPS=50
TX_VERSION=V0
COMPUTE_UNIT_PRICE_MICRO_LAMPORTS=50000

BS58_PRIVATE_KEY=your_base58_encoded_private_key
# SOLANA_KEYPAIR_PATH=/Users/masion/.config/solana/id.json

EXECUTE=false
```

## Token Selection

```mermaid
flowchart TD
    A["INPUT_TOKEN / OUTPUT_TOKEN"] --> B{"Built in?"}
    B -->|Yes| C["Use local mint + decimals"]
    B -->|No| D["Set INPUT_MINT / OUTPUT_MINT"]
    D --> E["Set INPUT_DECIMALS / OUTPUT_DECIMALS"]
    C --> F["Convert AMOUNT to raw units"]
    E --> F
    F --> G["Send raw amount to selected DEX"]
```

Built-in symbols:

| Symbol | Decimals |
| --- | ---: |
| `SOL` | 9 |
| `USDC` | 6 |
| `USDT` | 6 |
| `RAY` | 6 |
| `JUP` | 6 |

`AMOUNT=0.1` is converted using the input token decimals. Use `AMOUNT_RAW` when exact raw units are needed; `AMOUNT_RAW` takes priority over `AMOUNT`.

## DEX Selection

| Value | Integration |
| --- | --- |
| `DEX=raydium` | Raydium Trade API. Builds signed-ready transactions and sends them through `RPC_URL` only when execution is enabled. |
| `DEX=jupiter` | Jupiter Swap API. Uses Jupiter order/execute flow. Requires `JUPITER_API_KEY`. |

## Run

Dry-run first. This builds and signs the transaction locally, but does not broadcast:

```bash
cargo run
```

Submit once without changing `.env`:

```bash
cargo run -- --execute
```

Or enable execution in `.env`:

```dotenv
EXECUTE=true
```

Then run:

```bash
cargo run
```

## Safety Notes

```mermaid
flowchart LR
    SECRET[".env secrets"] --> IGNORE[".gitignore"]
    IGNORE --> LOCAL["Local only"]
    LOCAL --> SIGN["Transactions signed locally"]
    SIGN --> EXEC{"EXECUTE?"}
    EXEC -->|false| DRY["No broadcast"]
    EXEC -->|true| SEND["Broadcast signed transaction"]
```

- `.env` is ignored by Git and should contain private keys/API keys only locally.
- `EXECUTE=false` is the default safe mode.
- `BS58_PRIVATE_KEY` takes priority over `SOLANA_KEYPAIR_PATH` when both are set.
- `BS58_PRIVATE_KEY` can be either a 64-byte Solana keypair or a 32-byte seed encoded as base58.
