use std::fmt::Display;

use borsh::{BorshDeserialize as Deserialize, BorshSerialize as Serialize};
use bytemuck::{Pod, Zeroable};
use fixed::types::I80F48;
use shank::ShankAccount;

#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    PartialOrd,
    Ord,
    Zeroable,
    Pod,
    Deserialize,
    Serialize,
    ShankAccount,
)]
#[repr(transparent)]
pub struct WrappedI80F48 {
    pub value: [u8; 16],
}

impl WrappedI80F48 {
    pub const ZERO: Self = Self { value: [0u8; 16] };

    pub fn checked_add<T>(&self, rhs: T) -> Option<WrappedI80F48>
    where
        T: Into<I80F48>,
    {
        let lhs: I80F48 = (*self).into();
        let rhs: I80F48 = rhs.into();
        lhs.checked_add(rhs).map(WrappedI80F48::from)
    }

    pub fn checked_sub<T>(&self, rhs: T) -> Option<WrappedI80F48>
    where
        T: Into<I80F48>,
    {
        let lhs: I80F48 = (*self).into();
        let rhs: I80F48 = rhs.into();
        lhs.checked_sub(rhs).map(WrappedI80F48::from)
    }
}

impl From<I80F48> for WrappedI80F48 {
    fn from(i: I80F48) -> Self {
        Self {
            value: i.to_le_bytes(),
        }
    }
}

impl From<WrappedI80F48> for I80F48 {
    fn from(w: WrappedI80F48) -> Self {
        Self::from_le_bytes(w.value)
    }
}

impl PartialEq for WrappedI80F48 {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for WrappedI80F48 {}

impl From<u64> for WrappedI80F48 {
    fn from(val: u64) -> Self {
        I80F48::from_num(val).into()
    }
}

impl From<WrappedI80F48> for u64 {
    fn from(w: WrappedI80F48) -> Self {
        let i: I80F48 = w.into();
        i.to_num::<u64>()
    }
}

impl Display for WrappedI80F48 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let i: I80F48 = (*self).into();
        write!(f, "{}", i)
    }
}