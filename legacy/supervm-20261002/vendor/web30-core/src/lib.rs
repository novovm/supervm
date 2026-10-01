//! WEB30 Token 标准参考实现
//!
//! 提供下一代代币标准，具备 MVCC 并行、跨链原生、隐私保护等特性。

pub mod amm;
pub mod bonds;
pub mod cdp;
pub mod cross_chain;
pub mod dividend_pool;
pub mod e2e_integration;
pub mod economic_integration;
pub mod foreign_payment;
pub mod foreign_payment_impl;
pub mod governance;
pub mod mainnet_token;
pub mod mainnet_token_impl;
pub mod nav_redemption;
pub mod privacy;
pub mod token;
pub mod treasury;
pub mod treasury_impl;
pub mod types;

pub use token::WEB30Token;
pub use types::*;

#[cfg(test)]
mod tests;
