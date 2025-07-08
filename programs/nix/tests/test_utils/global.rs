use crate::test_utils::send_tx_with_retry;
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
};


use nix::{
    program::{
        get_dynamic_value, global_create_instruction::global_create_instruction
    },
    state::{GlobalFixed, GlobalValue},
    validation::{
        get_global_address,
        
    },
};

use std::{cell::RefCell, rc::Rc};

pub struct GlobalFixture {
    pub context: Rc<RefCell<ProgramTestContext>>,
    pub key: Pubkey,
    pub mint_key: Pubkey,
    pub global: GlobalValue,
}

impl GlobalFixture {
    pub async fn new_with_token_program(
        context: Rc<RefCell<ProgramTestContext>>,
        mint: &Pubkey,
        token_program: &Pubkey,
    ) -> Self {
        let (global_key, _global_bump) = get_global_address(mint);
        let payer: Pubkey = context.borrow().payer.pubkey();
        let payer_keypair: Keypair = context.borrow().payer.insecure_clone();

        let context_ref: Rc<RefCell<ProgramTestContext>> = Rc::clone(&context);

        let create_global_ix: Instruction =
            global_create_instruction(&global_key, &mint, &token_program, &payer, );

        send_tx_with_retry(
            Rc::clone(&context),
            &[create_global_ix],
            Some(&payer),
            &[&payer_keypair, &payer_keypair],
        )
        .await
        .unwrap();

        // Dummy default value. Not valid until reload.
        GlobalFixture {
            context: context_ref,
            key: global_key,
            mint_key: *mint,
            global: GlobalValue {
                fixed: GlobalFixed::new_empty(mint),
                dynamic: Vec::new(),
            },
        }
    }

    pub async fn new(context: Rc<RefCell<ProgramTestContext>>, mint: &Pubkey) -> Self {
        GlobalFixture::new_with_token_program(context, mint, &spl_token::id()).await
    }

    pub async fn reload(&mut self) {
        let global_account: Account = self
            .context
            .borrow_mut()
            .banks_client
            .get_account(self.key)
            .await
            .unwrap()
            .unwrap();

        let global: GlobalValue = get_dynamic_value(global_account.data.as_slice());
        self.global = global;
    }
}

// Note: GlobalFixture cannot implement Clone because MarginfiAccountFixture doesn't implement Clone
// If you need to duplicate a GlobalFixture, create a new one using GlobalFixture::new
