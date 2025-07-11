# Nix Protocol

*A Decentralized Lending Orderbook Protocol*

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](https://github.com/your-repo/nix-protocol)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue)](LICENSE)

Nix Protocol is a decentralized lending platform that combines an orderbook system with a lending protocol, designed to optimize capital efficiency by eliminating idle liquidity. Inspired by Morpho, Nix enables users to achieve improved interest rates through peer-to-peer matching: 

**How Rate Optimization Works:**
- If the underlying protocol offers 5% APR for lenders and charges 7% APR for borrowers
- Lenders can place orders above 5% (but below 7%) to earn more
- Borrowers can place orders below 7% (but above 5%) to pay less
- When matched at, say, 6% - both parties benefit:
  - Lenders earn 6% instead of 5% (+20% improvement)
  - Borrowers pay 6% instead of 7% (-14% savings)

## Current Roadmap Implementations

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

## Overview

Nix Protocol operates as a lending orderbook built on top of [Manifest](https://github.com/CKS-Systems/manifest), leveraging its unlimited orderbook architecture to create an efficient peer-to-peer lending marketplace. The protocol currently integrates with MarginFi as its underlying lending protocol, though the core architecture is designed to be adaptable to any lending protocol.

### Key Features

- **Orderbook-Based Lending**: Direct peer-to-peer lending through an efficient orderbook system
- **Capital Efficiency**: Deposits directly earn yield from underlying protocols via asset shares that accrue interest over time. Borrowers can reinvest collateral for additional yield generation
- **Fallback Integration**: Seamless fallback to underlying protocols (MarginFi) when no orders are available
- **Dual-Tree Architecture**: Optimized for bidirectional asset lending with specialized orderbook trees
- **Global Orders**: Capital-efficient orders that can be used across multiple markets
- **Risk Management**: Robust collateral management with buffer mechanisms to prevent bad debt during liquidation events.
- **Fee Structure**: No matching fees - only a small protocol fee (e.g., 0.2%) added to borrower rates on successful P2P loans

## Architecture

### Why Manifest?

Nix Protocol is built on top of Manifest for several key advantages that make it ideal for a lending orderbook:

- **No Matching Fees**: Zero fees for order matching on Manifest, keeping costs low for users
- **Atomic Lot Sizes**: Fine-grained price expression for optimal lending rate discovery
- **Capital Efficient Order Types**: P2P2Pool orders enable borrowers to reinvest their collateral for yield generation
- **Low Creation Costs**: Only 0.007 SOL to create a market vs 2-3+ SOL on other platforms
- **Composable Architecture**: Core vs wrapper design allows customization for lending-specific features
- **Global Orders**: Enables capital reuse across multiple lending markets
- **High Performance**: Optimized compute unit usage for frequent market making operations
- **Hypertree Data Structure**: Optimal hypertree red-black tree Data structure implementation for efficient orderbook operations and storage
- **Reverse Orders**: Borrowers can immediately lend their borrowed amounts at a specified spread for additional yield

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

## Technical Details

### Order Types

#### Standard Orders
- **Ask Orders**: Lend assets at specified rates
- **Bid Orders**: Borrow assets at specified rates

#### Global Orders
Global orders enable capital efficiency by allowing the same collateral to back orders across multiple markets. However, they are restricted to lenders only to prevent undercollateralization vulnerabilities.

**Why Global Orders Can't Be Bid Orders:**
A global order works by having one global deposit that can be used across multiple markets. For lenders, this makes sense as they can lend the same capital across different markets. However, for borrowers, this would create a vulnerability where they could borrow from multiple markets using the same collateral, leading to undercollateralization.

**How Global Order Matching Works:**
When a lender places global orders across multiple markets (e.g., SOL/USDC and BONK/USDC):
- The first order to get filled claims the entire global liquidity
- All other orders in different market pairs are immediately invalidated and cancelled
- A 5000 lamports fee is charged for global orders (refundable - given to the taker who removes invalidated orders)

**No Partial Fills for Security:**
Global orders are treated as atomic (all-or-nothing) orders. Here's why partial fills would be dangerous:

*Example Attack Scenario:*
1. User deposits $1,000 in a global account
2. Places two global lend orders:
   - Lend $1,000 on SOL-USDC market
   - Lend $1,000 on ETH-USDC market
3. If partial fills were allowed:
   - SOL-USDC could fill $500
   - ETH-USDC could fill $700 simultaneously
   - Result: $1,200 extracted from only $1,000 deposited

By enforcing atomic fills, we ensure that only one complete order can execute, preventing any possibility of over-lending from the global account.

#### P2P2Pool Orders
This capital-efficient order type allows borrowers to reinvest their collateral into yield-generating strategies. Instead of idle collateral, borrowers can earn yields that help offset their borrowing costs.

#### Reverse Orders
Borrowers can automatically place lend orders for their borrowed amounts at a specified spread. For example, if borrowing at 6%, they could immediately lend at 6.5%, capturing the spread as profit.

### Risk Management

The protocol implements several layers of risk management:

1. **Oracle Integration**: Uses the same oracles as underlying protocols to maintain consistent pricing
2. **Collateral Buffers**: Maintains collateral values above minimum requirements
3. **Liquidation Protection**: Prevents premature liquidation through buffer mechanisms
4. **Asset Tag Compliance**: Respects MarginFi's asset tag and risk tier restrictions

### Fee Model

Nix Protocol implements a minimal fee structure:
- **No Matching Fees**: Order matching is completely free
- **Protocol Fee**: Small percentage (e.g., 0.2%) added to borrower rates only on successful P2P matches
- **Example**: If matched at 5% APR, borrower pays 5.2% with a 0.2% protocol fee

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