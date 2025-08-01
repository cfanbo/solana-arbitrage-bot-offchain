# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Solana arbitrage bot built in Rust that uses Jupiter Aggregator API to find and execute profitable arbitrage opportunities. The bot continuously monitors token price differences between SOL and USDC, executing trades when profitable opportunities are discovered.

## Build and Development Commands

```bash
# Build the project
cargo build

# Build in release mode
cargo build --release

# Run the bot (default behavior)
cargo run

# Run with specific command
cargo run -- run

# Check for updates
cargo run -- update

# Check version
cargo run -- version

# Run tests (uses tempfile for testing)
cargo test
```

## Configuration

The bot requires a `config.toml` file in the project root. Copy from the example:
```bash
cp config.example.toml config.toml
```

Key configuration sections:
- **swap**: Token pairs and amounts (input_mint, output_mint, input_amount, slippage_bps)
- **jito**: Bundle submission settings for MEV protection
- **ips**: Multiple IP support to avoid rate limiting from Jupiter API

## Architecture

### Core Components

- **main.rs**: CLI entry point with subcommands (run, update, version)
- **engine.rs**: Main arbitrage engine containing the trading logic
- **config.rs**: Configuration management with TOML parsing
- **http_client.rs**: HTTP client with IP rotation for Jupiter API calls
- **blockhash.rs**: Caches recent blockhashes for transaction building
- **types.rs**: Data structures for Jupiter API requests/responses
- **util.rs**: Utility functions for keypair loading, profit calculations
- **constants.rs**: Program IDs and configuration constants

### Trading Flow

1. **Quote Fetching**: Engine continuously requests quotes from Jupiter API for both directions (SOL→USDC and USDC→SOL)
2. **Profit Calculation**: Compares output amounts to determine if arbitrage is profitable
3. **Transaction Building**: Constructs Solana transactions with proper ALT (Address Lookup Table) handling
4. **Submission**: Either submits via Jito bundles (MEV protection) or regular Solana RPC

### Key Features

- **Multi-IP Support**: Rotates through multiple IPs to avoid rate limiting
- **Jito Integration**: Optional bundle submission for MEV protection
- **Profit Verification**: On-chain profit checking before transaction execution
- **Exponential Backoff**: Retry logic for API failures
- **Real-time Monitoring**: Continuous price monitoring with configurable frequency

### Error Handling

The bot handles common trading errors gracefully:
- Insufficient funds (custom program error 0x1)
- Failed arbitrage (custom program error 0x64)  
- Rate limiting with exponential backoff
- Transaction size validation (1232 byte limit)

## Development Notes

- Uses `solana-sdk` version 2.2.x for Solana blockchain interaction
- Requires `jito-sdk-rust` for bundle submission functionality
- HTTP timeouts and latency thresholds are configurable
- Supports both simulated and real transaction execution modes
- Transaction signing uses Ed25519 keypairs loaded from various formats