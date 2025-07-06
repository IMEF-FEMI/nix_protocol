use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use hypertree::{
    DataIndex, FreeList, Get, HyperTreeReadOperations, HyperTreeWriteOperations, PodBool,
    RedBlackTree, RedBlackTreeReadOnly, NIL,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use shank::ShankType;
use solana_program::{entrypoint::ProgramResult, program_error::ProgramError, pubkey::Pubkey};
use static_assertions::const_assert_eq;
use std::{cmp::Ordering, mem::size_of};

use crate::{
    program::NixError,
    quantities::WrappedI80F48,
    require,
    state::{
        DerefOrBorrowMut, DynamicAccount, ACTIVE_LOAN_SIZE, MARKET_LOANS_FIXED_SIZE,
        MARKET_LOAN_BLOCK_SIZE, MARKET_LOAN_FREE_LIST_BLOCK_SIZE, MAX_ACTIVE_LOANS,
    },
    validation::NixAccount,
};

#[repr(C, packed)]
#[derive(Default, Copy, Clone, Pod, Zeroable)]
struct MarketLoansUnusedFreeListPadding {
    _padding: [u64; 10],
    _padding2: [u8; 12],
}
// 4 bytes are for the free list, rest is payload.
const_assert_eq!(
    size_of::<MarketLoansUnusedFreeListPadding>(),
    MARKET_LOAN_FREE_LIST_BLOCK_SIZE
);

#[derive(
    Debug,
    BorshDeserialize,
    BorshSerialize,
    PartialEq,
    Clone,
    Copy,
    ShankType,
    IntoPrimitive,
    TryFromPrimitive,
)]
#[repr(u8)]
pub enum LoanStatus {
    Active = 0,
    Repaid = 1,
    Defaulted = 2,
    Liquidated = 3,
}
unsafe impl bytemuck::Zeroable for LoanStatus {}
unsafe impl bytemuck::Pod for LoanStatus {}
impl Default for LoanStatus {
    fn default() -> Self {
        LoanStatus::Active
    }
}
#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable, Pod)]
pub struct MarketLoansFixed {
    /// Discriminant for identifying this account type.
    pub discriminant: u64,
    /// The market this loan ledger belongs to.
    pub market: Pubkey,
    /// The sequence number for the next loan, ensuring unique loan IDs.
    pub loan_sequence_number: u64,
    /// Root of the Hypertree storing active loans, indexed by sequence number.
    pub active_loans_root_index: DataIndex,
    /// LinkedList representing all free blocks for new loans.
    pub free_list_head_index: DataIndex,
    /// Number of bytes allocated for loan records.
    pub num_bytes_allocated: u32,
    /// The number of active loans currently stored.
    /// Padding to ensure 8-byte alignment.
    _padding: [u8; 4],
    pub num_active_loans: u64,
}

const_assert_eq!(
    size_of::<MarketLoansFixed>(),
    8  +  // discriminant
    32 +  // market
    8 +  // loan_sequence_number
    4 +   // loans_root_index
    4 +   // free_list_head_index
    4 +   // num_bytes_allocated 
    4 +   // _padding
    8 // num_active_loans
);
const_assert_eq!(size_of::<MarketLoansFixed>(), MARKET_LOANS_FIXED_SIZE);
const_assert_eq!(size_of::<MarketLoansFixed>() % 8, 0);

impl Get for MarketLoansFixed {}
impl NixAccount for MarketLoansFixed {
    fn verify_discriminant(&self) -> ProgramResult {
        let expected_discriminant: u64 =
            crate::utils::get_discriminant::<MarketLoansFixed>().unwrap();

        require!(
            self.discriminant == expected_discriminant,
            ProgramError::InvalidAccountData,
            "Invalid market discriminant actual: {} expected: {}",
            self.discriminant,
            expected_discriminant
        )?;
        Ok(())
    }
}

impl MarketLoansFixed {
    pub fn new_empty(market: Pubkey) -> Self {
        MarketLoansFixed {
            discriminant: crate::utils::get_discriminant::<MarketLoansFixed>().unwrap(),
            market,
            loan_sequence_number: 0,
            active_loans_root_index: NIL,
            free_list_head_index: NIL,
            num_bytes_allocated: 0,
            _padding: [0u8; 4],
            num_active_loans: 0,
        }
    }
    pub fn has_free_block(&self) -> bool {
        self.free_list_head_index != NIL
    }
}
#[repr(C)]
#[derive(Default, Debug, Copy, Clone, Zeroable, Pod)]
pub struct ActiveLoan {
    pub sequence_number: u64,
    pub lender_index: DataIndex,
    pub borrower_index: DataIndex,
    pub is_lender_global: PodBool,
    pub status: LoanStatus,
    pub is_liability_base_a: PodBool,
    _padding: [u8; 5],
    pub collateral_shares: WrappedI80F48,
    pub liability_shares: WrappedI80F48,
    pub rate_bps: u16,
    _padding2: [u8; 6],
    pub start_timestamp: i64,
    pub last_updated_slot: i64,
}
const_assert_eq!(size_of::<ActiveLoan>(), ACTIVE_LOAN_SIZE);
const_assert_eq!(size_of::<ActiveLoan>() % 8, 0);

// Required for Hypertree
impl Ord for ActiveLoan {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sequence_number.cmp(&other.sequence_number)
    }
}
impl PartialOrd for ActiveLoan {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for ActiveLoan {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_number == other.sequence_number
    }
}
impl Eq for ActiveLoan {}
impl std::fmt::Display for ActiveLoan {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.sequence_number)
    }
}

impl ActiveLoan {
    pub fn new_empty(
        is_liability_base_a: bool,
        lender_index: DataIndex,
        borrower_index: DataIndex,
        is_lender_global: bool,
        collateral_shares: WrappedI80F48,
        liability_shares: WrappedI80F48,
        rate_bps: u16,
        start_timestamp: i64,
        last_updated_slot: i64,
    ) -> Self {
        ActiveLoan {
            sequence_number: 0,
            lender_index,
            borrower_index,
            is_lender_global: PodBool::from(is_lender_global),
            is_liability_base_a: PodBool::from(is_liability_base_a),
            status: LoanStatus::Active,
            _padding: [0u8; 5],
            collateral_shares,
            liability_shares,
            rate_bps,
            _padding2: [0u8; 6],
            start_timestamp,
            last_updated_slot,
        }
    }
    pub fn set_sequence_number(&mut self, sequence_number: u64) {
        self.sequence_number = sequence_number
    }
}
pub type ActiveLoanTree<'a> = RedBlackTree<'a, ActiveLoan>;
pub type ActiveLoanTreeReadOnly<'a> = RedBlackTreeReadOnly<'a, ActiveLoan>;

/// Fully owned Global, used in clients that can copy.
pub type MarketLoansValue = DynamicAccount<MarketLoansFixed, Vec<u8>>;
/// Full MarketLoans reference type.
pub type MarketLoansRef<'a> = DynamicAccount<&'a MarketLoansFixed, &'a [u8]>;
/// Full MarketLoans reference type.
pub type MarketLoansRefMut<'a> = DynamicAccount<&'a mut MarketLoansFixed, &'a mut [u8]>;

impl<Fixed: DerefOrBorrowMut<MarketLoansFixed>, Dynamic: DerefOrBorrowMut<[u8]>>
    DynamicAccount<Fixed, Dynamic>
{
    fn borrow_mut_market_loans(&mut self) -> MarketLoansRefMut {
        MarketLoansRefMut {
            fixed: self.fixed.deref_or_borrow_mut(),
            dynamic: self.dynamic.deref_or_borrow_mut(),
        }
    }

    /// Expands the MarketLoans account by n block, adding the new space
    /// to the free list for future loan records.
    pub fn expand_loan_account(&mut self, n: u32) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut_market_loans();
        let mut free_list: FreeList<MarketLoansUnusedFreeListPadding> =
            FreeList::new(dynamic, fixed.free_list_head_index);

        free_list.add(fixed.num_bytes_allocated);
        fixed.num_bytes_allocated += n * MARKET_LOAN_BLOCK_SIZE as u32;
        fixed.free_list_head_index = free_list.get_head();
        Ok(())
    }

    pub fn add_loan(&mut self, loan_record: ActiveLoan) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut_market_loans();
        let free_address: DataIndex = get_free_address_on_market_loans_fixed(fixed, dynamic);

        let mut loan_tree: ActiveLoanTree =
            ActiveLoanTree::new(dynamic, fixed.active_loans_root_index, NIL);

        loan_tree.insert(free_address, loan_record);
        fixed.active_loans_root_index = loan_tree.get_root_index();

        require!(
            fixed.num_active_loans < MAX_ACTIVE_LOANS,
            NixError::MaxActiveLoansExceeded,
            "Maximum number of active loans exceeded {}",
            MAX_ACTIVE_LOANS
        )?;

        fixed.num_active_loans += 1;

        Ok(())
    }

    /// Add multiple loans to the active loans tree.
    pub fn add_loans(&mut self, loan_records: &[ActiveLoan]) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut_market_loans();

        require!(
            fixed.num_active_loans + (loan_records.len() as u64) <= MAX_ACTIVE_LOANS,
            NixError::MaxActiveLoansExceeded,
            "Adding {} loans would exceed the maximum number of active loans {}",
            loan_records.len(),
            MAX_ACTIVE_LOANS
        )?;

        for loan_record in loan_records {
            let free_address: DataIndex = get_free_address_on_market_loans_fixed(fixed, dynamic);
            let mut loan_tree: ActiveLoanTree =
                ActiveLoanTree::new(dynamic, fixed.active_loans_root_index, NIL);
            loan_tree.insert(free_address, *loan_record);
            fixed.active_loans_root_index = loan_tree.get_root_index();
            fixed.num_active_loans += 1;
        }

        Ok(())
    }

    /// Remove a loan from the active loans tree and free its slot.
    pub fn remove_loan(&mut self, sequence_number: u64) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut_market_loans();

        // Find the loan index by sequence_number
        let mut loan_tree: ActiveLoanTree =
            ActiveLoanTree::new(dynamic, fixed.active_loans_root_index, NIL);

        // Create a dummy loan to search by sequence_number
        let search_loan = ActiveLoan {
            sequence_number,
            ..Default::default()
        };
        let loan_index = loan_tree.lookup_index(&search_loan);

        require!(
            loan_index != NIL,
            NixError::InvalidFreeList,
            "Loan with sequence_number {} not found",
            sequence_number
        )?;

        // Remove from tree
        loan_tree.remove_by_index(loan_index);
        fixed.active_loans_root_index = loan_tree.get_root_index();

        // Free the slot
        let mut free_list: FreeList<MarketLoansUnusedFreeListPadding> =
            FreeList::new(dynamic, fixed.free_list_head_index);
        free_list.add(loan_index);
        fixed.free_list_head_index = free_list.get_head();

        // Decrement active loans count
        fixed.num_active_loans = fixed.num_active_loans.wrapping_sub(1);

        Ok(())
    }
}

fn get_free_address_on_market_loans_fixed(
    fixed: &mut MarketLoansFixed,
    dynamic: &mut [u8],
) -> DataIndex {
    let mut free_list: FreeList<MarketLoansUnusedFreeListPadding> =
        FreeList::new(dynamic, fixed.free_list_head_index);
    let free_address: DataIndex = free_list.remove();
    fixed.free_list_head_index = free_list.get_head();
    free_address
}
