use anyhow::Ok;
use test_utilities::test::{BankMint, TestSettings};

use crate::test_utils::NixTestFixture;
use test_case::test_case; 

#[test_case(&BankMint::SolSwbPull, &BankMint::Usdc)] 
#[tokio::test]
async fn create_market(
    base_a_mint: &BankMint, 
    base_b_mint: &BankMint,
) -> anyhow::Result<()> {
    let fixture = NixTestFixture::new(Some(TestSettings::all_banks_payer_not_admin()), base_a_mint, base_b_mint).await;
    Ok(())
}