use pinocchio::account_info::AccountInfo;

pub const MAX_LOAN_PAIRS: usize = 10;
pub const FEE: u128 = 500;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LoanData<'a> {
    pub protocol_token_accounts: &'a AccountInfo,
    pub balance: u64,
}

pub fn get_token_amount(data: &[u8]) -> u64 {
    unsafe { *(data.as_ptr().add(64) as *const u64) }
}