use pinocchio::account_info::AccountInfo;

pub const MAX_LOAN_PAIRS: usize = 10;
pub const FEE: u128 = 500;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LoanData<'a> {
    pub protocol_token_accounts: &'a AccountInfo,
    pub borrower_token_accounts: &'a AccountInfo,
}