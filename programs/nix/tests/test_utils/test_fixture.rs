use super::global::GlobalFixture;
use anchor_lang::{prelude::AccountInfo, Discriminator};
use bincode::deserialize;
use borsh::{BorshDeserialize, BorshSerialize};
use nix::{
    program::{
        claim_seat_instruction::claim_seat_instruction,
        create_market_instruction::create_market_instructions,
        create_market_loan_account_instruction::create_market_loan_account_instruction,
        global_add_trader_instruction::global_add_trader_instruction,
    },
    validation::get_nix_marginfi_account_address,
};
use solana_program::{hash::Hash, sysvar};
use solana_program_test::*;
use solana_sdk::{
    account::{Account, AccountSharedData}, clock::Clock, entrypoint::ProgramResult, instruction::Instruction, msg, program_pack::Pack, pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction
};

use test_utilities::{
    bank::BankFixture,
    marginfi_group::MarginfiGroupFixture,
    spl::{MintFixture, SupportedExtension, TokenAccountFixture},
    test::{
        BankMint, TestSettings, DEFAULT_PYUSD_TEST_BANK_CONFIG,
        DEFAULT_SB_PULL_SOL_TEST_REAL_BANK_CONFIG,
        DEFAULT_SB_PULL_WITH_ORIGINATION_FEE_BANK_CONFIG, DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
        DEFAULT_SOL_EQ_ISO_TEST_BANK_CONFIG, DEFAULT_SOL_TEST_BANK_CONFIG,
        DEFAULT_T22_WITH_FEE_TEST_BANK_CONFIG, DEFAULT_USDC_TEST_BANK_CONFIG, MNDE_MINT_DECIMALS,
        PYTH_MNDE_FEED, PYTH_PUSH_FULLV_FEED_ID, PYTH_PUSH_PARTV_FEED_ID, PYTH_PUSH_SOL_FULLV_FEED,
        PYTH_PUSH_SOL_PARTV_FEED, PYTH_PUSH_SOL_REAL_FEED, PYTH_PUSH_USDC_REAL_FEED,
        PYTH_PYUSD_FEED, PYTH_SOL_EQUIVALENT_FEED, PYTH_SOL_FEED, PYTH_T22_WITH_FEE_FEED,
        PYTH_USDC_FEED, PYUSD_MINT_DECIMALS, SOL_MINT_DECIMALS, SWITCH_PULL_SOL_REAL_FEED,
        T22_WITH_FEE_MINT_DECIMALS, USDC_MINT_DECIMALS,
    },
    transfer_hook::TEST_HOOK_ID,
};

use anyhow;
use pyth_solana_receiver_sdk::price_update::{PriceUpdateV2, VerificationLevel};
use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
    io::Error,
    rc::Rc,
};

pub struct NixTestFixture {
    pub context: Rc<RefCell<ProgramTestContext>>,
    pub base_a_mint_fixture: MintFixture,
    pub base_b_mint_fixture: MintFixture,
    pub base_a_global_fixture: GlobalFixture,
    pub base_b_global_fixture: GlobalFixture,
    pub base_a_marginfi_account: Pubkey,
    pub base_b_marginfi_account: Pubkey,
    pub group: MarginfiGroupFixture,
    pub base_a_bank_fixture: BankFixture,
    pub base_b_bank_fixture: BankFixture,

    pub payer_base_a_fixture: TokenAccountFixture,
    pub payer_base_b_fixture: TokenAccountFixture,
    pub base_a_token_program: Pubkey,
    pub base_b_token_program: Pubkey,

    pub market: Pubkey,
    pub second_keypair: Keypair,
    pub second_keypair_base_a_fixture: TokenAccountFixture,
    pub second_keypair_base_b_fixture: TokenAccountFixture,

    pub banks: HashMap<BankMint, BankFixture>,
}

impl NixTestFixture {
    pub async fn new(
        test_settings: Option<TestSettings>,
        base_a_mint: &BankMint,
        base_b_mint: &BankMint,
    ) -> NixTestFixture {
        Self::new_with_t22_extension(test_settings, &[], base_a_mint, base_b_mint).await
    }

    pub async fn new_with_t22_extension(
        test_settings: Option<TestSettings>,
        extensions: &[SupportedExtension],
        base_a_mint: &BankMint,
        base_b_mint: &BankMint,
    ) -> NixTestFixture {
        let mut program = ProgramTest::default();

        let mem_map_not_copy_feature_gate =
            solana_sdk::pubkey!("EenyoWx9UMXYKpR8mW5Jmfmy2fRjzUtM7NduYMY8bx33");
        program.deactivate_feature(mem_map_not_copy_feature_gate);

        program.prefer_bpf(true);

        // Add both MarginFi and Nix programs
        program.add_program("marginfi", marginfi::ID, None);
        program.add_program("nix", nix::ID, None);
        program.add_program("test_transfer_hook", TEST_HOOK_ID, None);

        let usdc_keypair = Keypair::new();
        let sol_keypair = Keypair::new();
        let sol_equivalent_keypair = Keypair::new();
        let mnde_keypair = Keypair::new();
        let usdc_t22_keypair = Keypair::new();
        let t22_with_fee_keypair = Keypair::new();

        // Add oracle accounts (copied from MarginFi TestFixture)
        program.add_account(
            PYTH_USDC_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_USDC_FEED.to_bytes(),
                1.0,
                USDC_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Full,
            ),
        );
        program.add_account(
            PYTH_PYUSD_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_PYUSD_FEED.to_bytes(),
                1.0,
                PYUSD_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Full,
            ),
        );
        program.add_account(
            PYTH_T22_WITH_FEE_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_T22_WITH_FEE_FEED.to_bytes(),
                0.5,
                T22_WITH_FEE_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Full,
            ),
        );
        program.add_account(
            PYTH_SOL_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_SOL_FEED.to_bytes(),
                10.0,
                SOL_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Full,
            ),
        );
        program.add_account(
            PYTH_SOL_EQUIVALENT_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_SOL_EQUIVALENT_FEED.to_bytes(),
                10.0,
                SOL_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Full,
            ),
        );
        program.add_account(
            PYTH_MNDE_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_MNDE_FEED.to_bytes(),
                10.0,
                MNDE_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Full,
            ),
        );
        program.add_account(
            PYTH_PUSH_SOL_FULLV_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_PUSH_FULLV_FEED_ID,
                10.0,
                SOL_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Full,
            ),
        );
        program.add_account(
            PYTH_PUSH_SOL_PARTV_FEED,
            test_utilities::utils::create_pyth_push_oracle_account(
                PYTH_PUSH_PARTV_FEED_ID,
                10.0,
                SOL_MINT_DECIMALS.into(),
                None,
                VerificationLevel::Partial { num_signatures: 5 },
            ),
        );
        program.add_account(
            PYTH_PUSH_USDC_REAL_FEED,
            test_utilities::utils::create_pyth_push_oracle_account_from_bytes(
                include_bytes!("data/pyth_push_usdc_price.bin").to_vec(),
            ),
        );
        program.add_account(
            PYTH_PUSH_SOL_REAL_FEED,
            test_utilities::utils::create_pyth_push_oracle_account_from_bytes(
                include_bytes!("data/pyth_push_sol_price.bin").to_vec(),
            ),
        );
        program.add_account(
            SWITCH_PULL_SOL_REAL_FEED,
            test_utilities::utils::create_switch_pull_oracle_account_from_bytes(
                include_bytes!("data/swb_pull_sol_price.bin").to_vec(),
            ),
        );

        let context = Rc::new(RefCell::new(program.start_with_context().await));

        {
            let ctx = context.borrow_mut();
            let mut clock: Clock = ctx.banks_client.get_sysvar().await.unwrap();
            clock.unix_timestamp = 0;
            ctx.set_sysvar(&clock);
        }

        let usdc_mint_f = MintFixture::new(
            Rc::clone(&context),
            Some(usdc_keypair),
            Some(USDC_MINT_DECIMALS),
        )
        .await;

        let sol_mint_f = MintFixture::new(
            Rc::clone(&context),
            Some(sol_keypair),
            Some(SOL_MINT_DECIMALS),
        )
        .await;

        let sol_equivalent_mint_f = MintFixture::new(
            Rc::clone(&context),
            Some(sol_equivalent_keypair),
            Some(SOL_MINT_DECIMALS),
        )
        .await;

        let _mnde_mint_f = MintFixture::new(
            Rc::clone(&context),
            Some(mnde_keypair),
            Some(MNDE_MINT_DECIMALS),
        )
        .await;

        let usdc_t22_mint_f = MintFixture::new_token_22(
            Rc::clone(&context),
            Some(usdc_t22_keypair),
            Some(USDC_MINT_DECIMALS),
            extensions,
        )
        .await;
        msg!("CARGO_MANIFEST_DIR");
        msg!((env!("CARGO_MANIFEST_DIR").to_string() + "/tests/test_utils/fixtures/pyUSD.json").as_str());
        let pyusd_mint_f = MintFixture::new_from_file(&context, "/tests/test_utils/fixtures/pyUSD.json");

        let t22_with_fee_mint_f = MintFixture::new_token_22(
            Rc::clone(&context),
            Some(t22_with_fee_keypair),
            Some(T22_WITH_FEE_MINT_DECIMALS),
            &[SupportedExtension::TransferFee],
        )
        .await;

        let tester_group = MarginfiGroupFixture::new(Rc::clone(&context)).await;
        let tester_group_key = tester_group.key;
        tester_group
            .set_protocol_fees_flag(test_settings.clone().unwrap_or_default().protocol_fees)
            .await;

        let mut banks = HashMap::new();
        if let Some(test_settings) = test_settings.clone() {
            for bank in test_settings.banks.iter() {
                let (bank_mint, default_config) = match bank.mint {
                    BankMint::Usdc => (&usdc_mint_f, *DEFAULT_USDC_TEST_BANK_CONFIG),
                    BankMint::Sol => (&sol_mint_f, *DEFAULT_SOL_TEST_BANK_CONFIG),
                    BankMint::SolSwbPull => {
                        (&sol_mint_f, *DEFAULT_SB_PULL_SOL_TEST_REAL_BANK_CONFIG)
                    }
                    BankMint::SolSwbOrigFee => (
                        &sol_mint_f,
                        *DEFAULT_SB_PULL_WITH_ORIGINATION_FEE_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent1 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent2 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent3 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent4 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent5 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent6 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent7 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent8 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::SolEquivalent9 => (
                        &sol_equivalent_mint_f,
                        *DEFAULT_SOL_EQUIVALENT_TEST_BANK_CONFIG,
                    ),
                    BankMint::T22WithFee => {
                        (&t22_with_fee_mint_f, *DEFAULT_T22_WITH_FEE_TEST_BANK_CONFIG)
                    }
                    BankMint::UsdcT22 => (&usdc_t22_mint_f, *DEFAULT_USDC_TEST_BANK_CONFIG),
                    BankMint::PyUSD => (&pyusd_mint_f, *DEFAULT_PYUSD_TEST_BANK_CONFIG),
                    BankMint::SolEqIsolated => {
                        (&sol_equivalent_mint_f, *DEFAULT_SOL_EQ_ISO_TEST_BANK_CONFIG)
                    }
                };

                banks.insert(
                    bank.mint.clone(),
                    tester_group
                        .try_lending_pool_add_bank(bank_mint, bank.config.unwrap_or(default_config))
                        .await
                        .unwrap(),
                );
            }
        };

        // so bank has been filled
        //group is set here
        //what we need now

        // pub base_a_token_program: Pubkey,
        // pub base_b_token_program: Pubkey,

        // pub market_fixture: MarketFixture,

        // market
        // market_loan_fixture
        let market_keypair = Keypair::new();
        let second_keypair = Keypair::new();
        let base_a_bank_fixture = banks.get(base_a_mint).clone().unwrap();
        let base_b_bank_fixture = banks.get(base_b_mint).clone().unwrap();
        let base_a_mint_fixture = base_a_bank_fixture.mint.clone();
        let base_b_mint_fixture = base_b_bank_fixture.mint.clone();
        let base_a_global_fixture = GlobalFixture::new_with_token_program(
            Rc::clone(&context),
            &base_a_bank_fixture.mint.key,
            &base_a_bank_fixture.mint.token_program,
        )
        .await;
        let base_b_global_fixture = GlobalFixture::new_with_token_program(
            Rc::clone(&context),
            &base_b_bank_fixture.mint.key,
            &base_b_bank_fixture.mint.token_program,
        )
        .await;

        let (base_a_marginfi_account, _) = get_nix_marginfi_account_address(
            &market_keypair.pubkey(),
            &base_a_bank_fixture.mint.key,
        );

        let (base_b_marginfi_account, _) = get_nix_marginfi_account_address(
            &market_keypair.pubkey(),
            &base_b_bank_fixture.mint.key,
        );

        let payer_base_a_fixture = TokenAccountFixture::new_with_keypair(
            Rc::clone(&context),
            &base_a_bank_fixture.mint.key,
            &context.borrow().payer.pubkey(),
            &Keypair::new(),
            &base_a_bank_fixture.mint.token_program,
        )
        .await;

        let payer_base_b_fixture = TokenAccountFixture::new_with_keypair(
            Rc::clone(&context),
            &base_b_bank_fixture.mint.key,
            &context.borrow().payer.pubkey(),
            &Keypair::new(),
            &base_b_bank_fixture.mint.token_program,
        )
        .await;
        let second_keypair_base_a_fixture = TokenAccountFixture::new_with_keypair(
            Rc::clone(&context),
            &base_a_bank_fixture.mint.key,
            &second_keypair.pubkey(),
            &Keypair::new(),
            &base_a_bank_fixture.mint.token_program,
        )
        .await;

        let second_keypair_base_b_fixture = TokenAccountFixture::new_with_keypair(
            Rc::clone(&context),
            &base_b_bank_fixture.mint.key,
            &second_keypair.pubkey(),
            &Keypair::new(),
            &base_b_bank_fixture.mint.token_program,
        )
        .await;

        let base_a_token_program = base_a_bank_fixture.mint.token_program;
        let base_b_token_program = base_b_bank_fixture.mint.token_program;

        let fixture = NixTestFixture {
            context: Rc::clone(&context),
            base_a_mint_fixture,
            base_b_mint_fixture,
            base_a_global_fixture,
            base_b_global_fixture,
            base_a_marginfi_account,
            base_b_marginfi_account,
            base_a_bank_fixture: base_a_bank_fixture.clone(),
            base_b_bank_fixture: base_b_bank_fixture.clone(),

            group: tester_group,

            payer_base_a_fixture,
            payer_base_b_fixture,
            second_keypair,
            second_keypair_base_a_fixture,
            second_keypair_base_b_fixture,
            base_a_token_program,
            base_b_token_program,
            market: market_keypair.pubkey(),
            banks: banks.clone(),
        };

        fixture
            .create_new_market(
                &market_keypair,
                &base_a_bank_fixture.mint.key,
                &base_b_bank_fixture.mint.key,
                &tester_group_key,
                &tester_group_key,
                &base_a_bank_fixture.key,
                &base_b_bank_fixture.key,
                &context.borrow().payer.pubkey(),
            )
            .await
            .unwrap();

        fixture
    }

    // Core MarginFi functionality
    pub fn context(&self) -> &Rc<RefCell<ProgramTestContext>> {
        &self.context
    }

    pub fn banks(&self) -> &HashMap<BankMint, BankFixture> {
        &self.banks
    }

    pub fn get_bank(&self, bank_mint: &BankMint) -> &BankFixture {
        self.banks
            .get(bank_mint)
            .unwrap_or_else(|| panic!("Bank not found for mint: {:?}", bank_mint))
    }

    pub fn payer(&self) -> Pubkey {
        self.context.borrow().payer.pubkey()
    }

    pub fn payer_keypair(&self) -> Keypair {
        Keypair::from_bytes(&self.context.borrow().payer.to_bytes()).unwrap()
    }

    pub fn set_time(&self, timestamp: i64) {
        let clock = Clock {
            unix_timestamp: timestamp,
            ..Default::default()
        };
        self.context.borrow_mut().set_sysvar(&clock);
    }

    pub async fn set_pyth_oracle_timestamp(&self, address: Pubkey, timestamp: i64) {
        let mut ctx = self.context.borrow_mut();

        let mut account = ctx
            .banks_client
            .get_account(address)
            .await
            .unwrap()
            .unwrap();

        let data = account.data.as_mut_slice();
        let mut price_update = PriceUpdateV2::deserialize(&mut &data[8..]).unwrap();

        price_update.price_message.publish_time = timestamp;
        price_update.price_message.prev_publish_time = timestamp;

        let mut data = vec![];
        let mut account_data = vec![];

        data.extend_from_slice(PriceUpdateV2::DISCRIMINATOR);

        price_update.serialize(&mut account_data).unwrap();

        data.extend_from_slice(&account_data);

        let mut aso = AccountSharedData::from(account);

        aso.set_data_from_slice(data.as_slice());

        ctx.set_account(&address, &aso);
    }

    pub async fn advance_time(&self, seconds: i64) {
        let mut clock: Clock = self
            .context
            .borrow_mut()
            .banks_client
            .get_sysvar()
            .await
            .unwrap();
        clock.unix_timestamp += seconds;
        self.context.borrow_mut().set_sysvar(&clock);
        self.context
            .borrow_mut()
            .warp_forward_force_reward_interval_end()
            .unwrap();
    }

    pub async fn get_minimum_rent_for_size(&self, size: usize) -> u64 {
        self.context
            .borrow_mut()
            .banks_client
            .get_rent()
            .await
            .unwrap()
            .minimum_balance(size)
    }

    pub async fn get_latest_blockhash(&self) -> Hash {
        self.context
            .borrow_mut()
            .banks_client
            .get_latest_blockhash()
            .await
            .unwrap()
    }

    pub async fn get_slot(&self) -> u64 {
        self.context
            .borrow_mut()
            .banks_client
            .get_root_slot()
            .await
            .unwrap()
    }

    pub async fn get_clock(&self) -> Clock {
        deserialize::<Clock>(
            &self
                .context
                .borrow_mut()
                .banks_client
                .get_account(sysvar::clock::ID)
                .await
                .unwrap()
                .unwrap()
                .data,
        )
        .unwrap()
    }

    pub async fn create_new_market(
        &self,
        market_keypair: &Keypair,
        base_a_mint: &Pubkey,
        base_b_mint: &Pubkey,
        base_a_group: &Pubkey,
        base_b_group: &Pubkey,
        base_a_bank: &Pubkey,
        base_b_bank: &Pubkey,
        admin: &Pubkey,
    ) -> anyhow::Result<Pubkey, BanksClientError> {
        let payer: Pubkey = self.context.borrow().payer.pubkey();
        let payer_keypair: Keypair = self.context.borrow().payer.insecure_clone();

        let create_market_ixs = create_market_instructions(
            &market_keypair.pubkey(),
            base_a_mint,
            base_b_mint,
            base_a_group,
            base_b_group,
            base_a_bank,
            base_b_bank,
            admin,
        )
        .unwrap();

        send_tx_with_retry(
            Rc::clone(&self.context),
            &create_market_ixs[..],
            Some(&payer),
            &[&payer_keypair, &market_keypair],
        )
        .await
        .unwrap();
        Ok(market_keypair.pubkey())
    }

    pub async fn create_market_loan_account(
        &self,
        admin: &Pubkey,
        market: &Pubkey,
    ) -> anyhow::Result<Pubkey, BanksClientError> {
        let market_loan_keypair: Keypair = Keypair::new();
        let payer: Pubkey = self.context.borrow().payer.pubkey();
        let payer_keypair: Keypair = self.context.borrow().payer.insecure_clone();

        let create_market_ixs =
            create_market_loan_account_instruction(admin, &market_loan_keypair.pubkey(), market)
                .unwrap();

        send_tx_with_retry(
            Rc::clone(&self.context),
            &create_market_ixs[..],
            Some(&payer),
            &[&payer_keypair, &market_loan_keypair],
        )
        .await
        .unwrap();
        Ok(market_loan_keypair.pubkey())
    }

    pub async fn claim_seat_for_keypair(
        &self,
        keypair: &Keypair,
    ) -> anyhow::Result<(), BanksClientError> {
        let claim_seat_ix: Instruction = claim_seat_instruction(&self.market, &keypair.pubkey());
        send_tx_with_retry(
            Rc::clone(&self.context),
            &[claim_seat_ix],
            Some(&keypair.pubkey()),
            &[keypair],
        )
        .await
    }

    pub async fn global_add_trader(
        &self,
        global_fixture_key: &Pubkey,
    ) -> anyhow::Result<(), BanksClientError> {
        self.global_add_trader_for_keypair(&self.payer_keypair(), global_fixture_key)
            .await
    }

    pub async fn global_add_trader_for_keypair(
        &self,
        keypair: &Keypair,
        global_fixture_key: &Pubkey,
    ) -> anyhow::Result<(), BanksClientError> {
        send_tx_with_retry(
            Rc::clone(&self.context),
            &[global_add_trader_instruction(
                global_fixture_key,
                &keypair.pubkey(),
            )],
            Some(&keypair.pubkey()),
            &[&keypair],
        )
        .await
    }

    pub async fn load_and_deserialize<T: anchor_lang::AccountDeserialize>(
        &self,
        address: &Pubkey,
    ) -> T {
        let account = self
            .context
            .borrow_mut()
            .banks_client
            .get_account(*address)
            .await
            .unwrap()
            .expect("Account not found");

        T::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub async fn get_and_deserialize<T: Pack>(
        context: Rc<RefCell<ProgramTestContext>>,
        pubkey: Pubkey,
    ) -> T {
        let context: RefMut<ProgramTestContext> = context.borrow_mut();
        loop {
            let account_or: Result<Option<Account>, BanksClientError> =
                context.banks_client.get_account(pubkey).await;
            if !account_or.is_ok() {
                continue;
            }
            let account_opt: Option<Account> = account_or.unwrap();
            if account_opt.is_none() {
                continue;
            }
            return T::unpack_unchecked(&mut account_opt.unwrap().data.as_slice()).unwrap();
        }
    }
    // Additional utility methods matching MarginFi TestFixture
    pub async fn try_load(
        &self,
        address: &Pubkey,
    ) -> anyhow::Result<Option<solana_sdk::account::Account>, solana_program_test::BanksClientError>
    {
        self.context
            .borrow_mut()
            .banks_client
            .get_account(*address)
            .await
    }

    pub fn get_bank_mut(&mut self, bank_mint: &BankMint) -> &mut BankFixture {
        self.banks.get_mut(bank_mint).unwrap()
    }

    pub async fn get_sufficient_collateral_for_outflow(
        &self,
        outflow_amount: f64,
        outflow_mint: &BankMint,
        collateral_mint: &BankMint,
    ) -> f64 {
        let outflow_bank = self.get_bank(outflow_mint);
        let collateral_bank = self.get_bank(collateral_mint);

        let outflow_mint_price = outflow_bank.get_price().await;
        let collateral_mint_price = collateral_bank.get_price().await;

        let collateral_amount = test_utilities::utils::get_sufficient_collateral_for_outflow(
            outflow_amount,
            outflow_mint_price,
            collateral_mint_price,
        );

        let decimal_scaling = 10.0_f64.powi(collateral_bank.mint.mint.decimals as i32);
        let collateral_amount =
            ((collateral_amount * decimal_scaling).round() + 1.) / decimal_scaling;

        test_utilities::utils::get_max_deposit_amount_pre_fee(collateral_amount)
    }
}

pub async fn send_tx_with_retry(
    context: Rc<RefCell<ProgramTestContext>>,
    instructions: &[Instruction],
    payer: Option<&Pubkey>,
    signers: &[&Keypair],
) -> Result<(), BanksClientError> {
    let mut context: RefMut<ProgramTestContext> = context.borrow_mut();

    loop {
        let blockhash_or: Result<Hash, Error> = context.get_new_latest_blockhash().await;
        if blockhash_or.is_err() {
            continue;
        }
        let tx: Transaction =
            Transaction::new_signed_with_payer(instructions, payer, signers, blockhash_or.unwrap());
        let result: Result<(), BanksClientError> =
            context.banks_client.process_transaction(tx).await;
        if result.is_ok() {
            break;
        }
        let error: BanksClientError = result.err().unwrap();
        match error {
            BanksClientError::RpcError(_rpc_err) => {
                // Retry on rpc errors.
                continue;
            }
            BanksClientError::Io(_io_err) => {
                // Retry on io errors.
                continue;
            }
            _ => {
                println!("Unexpected error: {:?}", error);
                return Err(error);
            }
        }
    }
    Ok(())
}
