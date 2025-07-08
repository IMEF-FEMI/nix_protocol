# Nix Protocol

*A Decentralized Lending Orderbook Protocol*

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](https://github.com/your-repo/nix-protocol)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue)](LICENSE)

Nix Protocol is a decentralized lending platform that combines an orderbook system with a lending protocol, designed to optimize capital efficiency by eliminating idle liquidity. Inspired by Morpho, Nix enables users to achieve improved interest rates: lenders can earn higher APY than traditional protocols offer, while borrowers can access loans at lower APY than they would typically pay.

## Overview

Nix Protocol operates as a lending orderbook built on top of [Manifest](https://github.com/CKS-Systems/manifest), leveraging its unlimited orderbook architecture to create an efficient peer-to-peer lending marketplace. The protocol currently integrates with MarginFi as its underlying lending protocol, though the core architecture is designed to be adaptable to any lending protocol.

### Key Features

- **Orderbook-Based Lending**: Direct peer-to-peer lending through an efficient orderbook system
- **Capital Efficiency**: Eliminates idle liquidity through active order matching
- **Fallback Integration**: Seamless fallback to underlying protocols (MarginFi) when no orders are available
- **Dual-Tree Architecture**: Optimized for bidirectional asset lending with specialized orderbook trees
- **Global Orders**: Capital-efficient orders that can be used across multiple markets
- **Risk Management**: Robust collateral management with buffer mechanisms to prevent liquidation

## Architecture

### Why Manifest?

Nix Protocol is built on top of Manifest for several key advantages that make it ideal for a lending orderbook:

- **Feeless Trading**: No trading fees forever on Manifest, reducing costs for lenders and borrowers
- **Atomic Lot Sizes**: Fine-grained price expression for optimal lending rate discovery
- **Capital Efficiency**: Built-in capital efficient order types perfect for lending markets
- **Low Creation Costs**: Only 0.007 SOL to create a market vs 2-3+ SOL on other platforms
- **Composable Architecture**: Core vs wrapper design allows customization for lending-specific features
- **Global Orders**: Enables capital reuse across multiple lending markets
- **High Performance**: Optimized compute unit usage for frequent market making operations
- **Hypertree Data Structure**: Optimal hypertree red-black tree Data structure implementation for efficient orderbook operations and storage

### Core Design Principles

#### Dual-Tree Orderbook System

Nix utilizes two distinct orderbook trees to handle bidirectional lending:

- **Tree A**: Base asset A as collateral, borrows asset B
- **Tree B**: Base asset B as collateral, borrows asset A

This design ensures that both assets can serve as either collateral or debt, creating a truly bidirectional lending market without the limitations of traditional base/quote asset structures.

#### MarginFi Integration

The protocol integrates with MarginFi through dedicated market accounts:

- **Asset A Account**: Holds asset A deposits and borrows asset B
- **Asset B Account**: Holds asset B deposits and borrows asset A

This approach addresses MarginFi's asset tag and risk tier limitations while maintaining compatibility with the underlying protocol's risk management systems.

#### Why One Giant MarginFi Account?

Rather than creating individual MarginFi accounts for each user, Nix uses consolidated accounts for several reasons:

1. **On-chain Matching Efficiency**: The matching engine cannot predetermine which orders will fill, making it impossible to know which accounts to include in advance
2. **Transaction Limits**: Solana's account limit per transaction would severely limit the scalability of the place_order function
3. **Cost Efficiency**: Reduces the overhead of managing thousands of individual accounts

#### Buffer Mechanism

To ensure borrower collateral remains sufficient, Nix implements a buffer system that maintains collateral values slightly above the underlying protocol requirements. This prevents orders from being removed due to collateral drops before matching can occur.

## Installation & Setup

### Prerequisites

- Rust 1.79 or later
- Solana CLI tools
- Node.js 16+ (for the incoming TypeScript SDK 😉)

### Building the Program

```bash
# Clone or fork the repository
git clone https://github.com/imef-femi/nix-protocol
cd nix-protocol

# Build the program
cargo build-sbf

# Run program tests
cargo test-sbf
```

### Running specific Tests

```bash
# Run all tests
cargo test-sbf

# Run specific test cases
cargo test-sbf cases::create_market::create_market
```

### Notes for Setup

If you encounter dependency issues, you may need to patch the `half` crate version:

```bash
cargo update -p half --precise 2.4.1
```


## Current Implementation Status

The following instructions are currently implemented:

- ✅ `CreateMarket`: Initialize new lending markets
- ✅ `CreateMarketLoanAccount`: Set up loan account structures
- ✅ `ClaimSeat`: Allocate trading seats for users
- ✅ `Deposit`: Deposit assets into the protocol
- ✅ `GlobalCreate`: Create global trading accounts
- ✅ `GlobalAddTrader`: Add traders to global accounts
- ✅ `GlobalDeposit`: Deposit into global accounts
- ✅ `PlaceOrder`: Place lending/borrowing orders
- ✅ `CancelOrder`: Cancel existing orders

## Roadmap

### Completed ✅
- [x] Core orderbook infrastructure (based on Manifest)
- [x] Market creation and management
- [x] Basic order placement and cancellation
- [x] MarginFi integration for fallback lending
- [x] Dual-tree orderbook architecture
- [x] Global order support
- [x] Deposit and withdrawal mechanisms

### In Progress 🟡
- [ ] Robust testing framework and comprehensive test coverage
- [ ] Advanced matching engine testing and edge case handling

### Planned 🔄
- [ ] Complete remaining orderbook functions
- [ ] Loan lifecycle management (repay, exit, liquidate)
- [ ] Interest accrual mechanisms
- [ ] Robust risk engine mirroring underlying protocol
- [ ] Collateral reinvestment functionality for yield optimization
- [ ] Enhanced admin functionalities
- [ ] End-to-end lifecycle testing
- [ ] Complete TypeScript SDK
- [ ] Testnet deployment
- [ ] Web interface and user dashboard

## Technical Details

### Order Types

#### Standard Orders
- **Ask Orders**: Lend assets at specified rates
- **Bid Orders**: Borrow assets at specified rates

#### Global Orders
Global orders enable capital efficiency by allowing the same collateral to back orders across multiple markets. However, they are restricted to lenders only to prevent undercollateralization vulnerabilities.

**Why Global Orders Can't Be Bid Orders:**
A global order works by having one global deposit that can be used across multiple markets. For lenders, this makes sense as they can lend the same capital across different markets. However, for borrowers, this would create a vulnerability where they could borrow from multiple markets using the same collateral, leading to undercollateralization.

### Risk Management

The protocol implements several layers of risk management:

1. **Oracle Integration**: Uses the same oracles as underlying protocols to maintain consistent pricing
2. **Collateral Buffers**: Maintains collateral values above minimum requirements
3. **Liquidation Protection**: Prevents premature liquidation through buffer mechanisms
4. **Asset Tag Compliance**: Respects MarginFi's asset tag and risk tier restrictions

## Contributing

We welcome contributions to Nix Protocol! Please read our contributing guidelines and submit pull requests for any improvements.

## Security

Nix Protocol takes security seriously. If you discover any security vulnerabilities, please report them responsibly by contacting our security team.

## License

This project is licensed under the GPL-3.0 License - see the [LICENSE](LICENSE) file for details.



## Acknowledgments

- Built on top of [Manifest](https://github.com/CKS-Systems/manifest) - The Unlimited Orderbook
- Integrates with [MarginFi](https://github.com/mrgnlabs/marginfi-v2) for underlying lending protocol
- Inspired by [Morpho](https://morpho.xyz) for capital-efficient lending design