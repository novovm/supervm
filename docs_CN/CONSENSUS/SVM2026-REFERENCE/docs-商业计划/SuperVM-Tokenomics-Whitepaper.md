# SuperVM Tokenomics & Financial System Whitepaper (Draft)

> Version: v0.1 (External Facing Draft)
> Date: 2025-11-17
> Status: For Review / Investor & Partner Preview

---

## 1. Introduction

### 1.1 Vision

SuperVM aims to build a next-generation programmable financial infrastructure on top of a high-performance WASM-first blockchain virtual machine. Beyond a simple gas token, the SuperVM mainnet token and its surrounding modules form a complete **decentralized financial system**, combining the roles of:

- A **decentralized central bank** (issuance, reserves, monetary policy)
- A **decentralized commercial bank** (deposits, lending, stablecoins)
- A **decentralized investment bank** (bonds, securitization, RWA)
- A **decentralized fund** (NAV, dividends, buyback & burn)

### 1.2 Design Principles

- **Sound Money**: Fixed M0 cap, long-term deflationary pressure, transparent supply schedule
- **Reserve-Backed**: Multi-currency reserves (BTC/ETH/USDT/BNB/SOL) and real-world assets (RWA)
- **Market-Based**: AMM-based price discovery, no hard pegs, floating market-driven pricing
- **Cashflow Oriented**: Daily dividend distribution to token holders from protocol revenue
- **Governance Ready**: 9-seat weighted council with delegation for small holders
- **Interoperable**: Multi-chain payment, cross-chain settlement, RWA bridge integration

---

## 2. Monetary Layers & Supply

### 2.1 Monetary Layers

SuperVM adopts a three-layer monetary structure:

- **M0 – Base Money**: Fixed-cap mainnet token (hard cap 100,000,000)
- **M1 – Circulating Money**: Unlocked M0 minus burned amount, actually circulating in the market
- **M2 – Credit Money**: M1 plus all outstanding stablecoins and credit instruments

Formally:

- M1 = Unlocked_M0 + Redistributed_Fees − Burned_Tokens
- M2 = M1 + Stablecoins (e.g. SVM-based USD stablecoin) + CDP debt positions

### 2.2 Initial Distribution & Vesting

- Total supply (M0): 100,000,000 tokens, minted once and never increased
- Typical allocation (illustrative):
  - 40% Mining / Node Incentives (10-year linear release)
  - 20% Ecosystem Fund (governance-controlled)
  - 15% Team (2-year lock, then 3-year linear vesting)
  - 10% Early Investors (1-year lock, then 2-year linear vesting)
  - 10% Treasury Reserve (used for foreign payment replacement to miners)
  - 5% Liquidity Bootstrapping (initial AMM pools)

### 2.3 Deflation Mechanisms

- **Gas Base Fee Burn**: A fixed portion of gas is burned per transaction
- **Fee Burn**: A portion of transaction/service fees is burned
- **Liquidation Penalties**: Part of liquidation penalty is burned
- **Treasury Buyback & Burn**: Governance-controlled buyback programs in specific conditions

Target band: **−3% ~ −5% net annual deflation** over the long term, with dynamic adjustment based on circulating supply and economic conditions.

---

## 3. Dual-Track Pricing & Rigid NAV Redemption

### 3.1 Two Prices for One Token

SuperVM introduces a dual-track pricing model:

1. **Market Price (P_market)**
   - Determined by AMM pools (Token/USDT, Token/BTC, Token/ETH, etc.)
   - Fully floating, 24/7, supply-demand driven

2. **Net Asset Value Price – NAV (P_NAV)**
   - Defined as:

     P_NAV = (Total Reserve Value in USD) / Circulating Supply (M1)

   - Reserves include: BTC, ETH, USDT, BNB, SOL and eventually RWA baskets

This establishes a **soft floor**: in normal conditions, rational actors will not sell significantly below NAV when a rigid redemption mechanism exists.

### 3.2 Rigid NAV Redemption

Token holders may choose to redeem at NAV via the treasury, subject to:

- Daily quota (e.g. 1% of circulating supply) to prevent bank-runs
- Settlement delay (e.g. T+7) to give the system time to rebalance
- Reserve ratio threshold (e.g. redemption paused if Reserve_Ratio < 50%)

Redemption flow:

1. User submits redemption request (amount of tokens, preferred currency: USDT/BTC/ETH)
2. System calculates current NAV from on-chain reserves and price oracles
3. Treasury schedules payout in chosen currency at NAV-equivalent value
4. Tokens are burned, reducing supply and increasing per-token backing

### 3.3 Arbitrage & Price Convergence

When market price deviates significantly from NAV:

- If P_market << P_NAV:
  - Arbitrageurs buy tokens cheaply on AMM
  - Redeem at NAV via treasury
  - Profit from P_NAV − P_market
  - Buying pressure lifts P_market upwards toward P_NAV

- If P_market >> P_NAV:
  - No risk-free arbitrage via NAV, but natural profit-taking and issuance of credit instruments (e.g. CDP) tend to push price downwards

Result: **P_market and P_NAV converge over time**, balancing speculative dynamics with fundamental backing.

---

## 4. Treasury Bonds (SVM Bonds)

### 4.1 Role of On-Chain Bonds

SuperVM introduces native on-chain bonds ("SVM Bonds") with three major functions:

1. **Treasury Funding**: Raise capital for ecosystem development without diluting token holders
2. **Monetary Policy Tool**: Expand or contract liquidity via bond issuance and buybacks
3. **Collateral Asset**: High-quality collateral within the CDP and lending system

### 4.2 Bond Parameters

Illustrative parameters:

- Face value: 100 USDT
- Issue price: 95 USDT (5% discount)
- Tenors: 1 / 3 / 5 years
- Coupon rates: 8% / 12% / 15% annually
- Coupon payment: Quarterly in mainnet tokens
- Principal repayment: At maturity in USDT (or equivalent stable reserve)

### 4.3 Use of Proceeds

- Ecosystem investments (e.g. high-quality dApps on SuperVM)
- Infrastructure support (node subsidies, RPC services)
- Liquidity management (initial and ongoing AMM pool injections)

### 4.4 Secondary Market & Yield Curve

- Bonds are freely tradable on-chain (bond/USDT, bond/SVM markets)
- Prices reflect market interest rates and credit risk
- A natural on-chain yield curve emerges, guiding capital allocation and policy decisions

### 4.5 Risk Management

- Conservative issuance: bond interest costs covered multiple times by protocol cash flows
- Bond insurance fund: a small portion of issuance reserved for potential restructuring
- Governance-controlled emergency procedures for extreme scenarios

---

## 5. CDP System & Native Stablecoin

### 5.1 Collateralized Debt Positions (CDP)

The CDP system allows users to lock various collateral types to borrow:

- A native USD-pegged stablecoin (name TBD, e.g. XXXX)
- Or additional mainnet tokens for leverage/liquidity purposes

Supported collateral tiers (illustrative):

- **Level 1**: Mainnet Token
  - Collateral ratio: 150%
  - Liquidation threshold: 130%

- **Level 2**: SVM Bonds
  - Collateral ratio: 120%
  - Liquidation threshold: 110%

- **Level 3**: Major Crypto (BTC, ETH)
  - Collateral ratio: 200%
  - Liquidation threshold: 150%

- **Level 4**: RWA Assets (tokenized real estate, invoices, etc.)
  - Collateral ratio: 250%
  - Liquidation threshold: 180%

### 5.2 Stablecoin Design

- Peg target: 1 unit = 1 USD
- Backing: over-collateralized debts across the above collateral tiers
- Stability fees: interest paid by borrowers, accruing to the protocol treasury
- Liquidation mechanism: under-collateralized positions are auctioned; penalties partly burned and partly sent to a risk fund

### 5.3 Comparison with MakerDAO

- Broader collateral set (including SVM bonds and native token)
- Integrated dual-track pricing (NAV + market) for additional stability
- Tight coupling with on-chain bonds and treasury operations

---

## 6. Real-World Assets (RWA)

### 6.1 Asset Types

SuperVM supports tokenization of:

- Real estate (commercial and residential)
- Public and private equities
- Gold and other precious metals
- Invoices and receivables
- Other yield-bearing assets (subject to legal frameworks)

### 6.2 Token Forms & Use Cases

- **RWA-Token (fungible)**: suitable for divisible assets (gold, index-like baskets, certain equity exposures)
- **RWA-NFT / 1155**: suitable for unique or low-quantity assets (single property, individual invoice)

They serve three roles:

1. **Tradable Assets**: RWA can be traded on-chain like stocks/funds (RWA/USDT, RWA/SVM pairs)
2. **Collateral**: RWA can be locked in CDPs to mint stablecoins or borrow mainnet tokens
3. **Yield Carriers**: income streams (rent, interest, dividends) can be distributed to RWA holders

### 6.3 External Protocol Integration

- Bridges to established RWA protocols (e.g. Centrifuge, Maple, RealT, Backed, etc.)
- Native RWA onboarding: legal agreements, custodians, auditors, and valuation providers

### 6.4 Risk & Legal Framework

- Third-party audits (Proof of Reserve, legal opinions)
- Valuation models (market prices, appraisals, discounted cash flow)
- Jurisdiction selection (e.g. Singapore, Switzerland)
- Enforceability (lien, foreclosure, and liquidation procedures)

---

## 7. Revenue Sharing & Incentives

### 7.1 80/20 Revenue Split

Protocol revenue (gas + fees + interest + penalties) is split approximately as:

- ~80% to token holders via the dividend pool
- ~20% to the foundation treasury for development and operations

### 7.2 Daily Dividend Mechanism

- Daily snapshot at a fixed time (e.g. UTC 00:00)
- Users claim dividends via an on-chain `claim()` function
- Unclaimed dividends accumulate and remain available

This design avoids gas-wasting mass distributions and encourages long-term holding.

### 7.3 Miner & Node Incentives with Foreign Currency Replacement

- External chain users pay in BTC/ETH/USDT for SuperVM services
- 100% of foreign payments go to the reserve pool
- Miners and validators are paid **in mainnet tokens** at the equivalent value
- Miners can choose to hold tokens for dividends, or swap back to foreign currency via AMM

---

## 8. Governance & Monetary Policy

### 8.1 Governance Structure

- 9-seat weighted council (founder, top holders, team, independent member)
- Voting power based on role and token holdings
- Delegation mechanism for small holders

### 8.2 Governable Parameters

- Bond issuance and buyback schedules
- CDP parameters (collateral ratios, liquidation penalties, stability fees)
- NAV redemption limits and rules
- Buyback & burn programs

### 8.3 Emergency Controls

- Circuit breakers for extreme volatility
- Temporary suspension of certain features (e.g. NAV redemption) under defined conditions
- On-chain recorded, time-bounded emergency powers

---

## 9. Risk Disclosures

- Market risk: token price volatility, liquidity risk
- Credit risk: bond defaults, CDP liquidations, RWA counterparty risk
- Technical risk: smart contract bugs, oracle failures, bridge exploits
- Regulatory risk: jurisdictional changes affecting bonds, stablecoins, and RWA

SuperVM mitigates these via conservative collateralization, transparent on-chain operations, independent audits, and modular design.

---

## 10. Roadmap & Outlook

- Phase T1: Core token, treasury, foreign payment, dividend and governance modules
- Phase T2: AMM, reserve pools, dual-track pricing, NAV calculator, redemption
- Phase T2.5: SVM Bonds issuance and bond market
- Phase T3: Dividend and governance front-ends
- Phase T3.5: CDP system and RWA integration
- Phase T4: Resource pricing oracles and economic dashboards

SuperVM’s tokenomics are designed to be sustainable over decades, aligning incentives between users, validators, investors, and builders, while bridging on-chain and off-chain assets into a coherent, programmable financial ecosystem.
