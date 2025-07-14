use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use crate::{get_token_amount, LoanData};

/// #Repay
/// 
/// Repay tokens to the protocol
/// 
/// Accounts:
/// 
/// 1. borrower:                        [signer, mut]
/// 2. loan:
/// 3. token_program:
/// 4. ..remaining accounts are token accounts from protocol
pub struct RepayAccounts<'a> {
    pub borrower: &'a AccountInfo,
    pub loan: &'a AccountInfo,
    pub token_accounts: &'a [AccountInfo],
}

impl<'a> TryFrom<&'a [AccountInfo]> for RepayAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [borrower, loan, _token_program, token_accounts @ ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        Ok(Self {
            borrower,
            loan,
            token_accounts,
        })
    }
}

pub struct Repay<'a> {
    pub accounts: RepayAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for Repay<'a> {
    type Error = ProgramError;
    
    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let accounts = RepayAccounts::try_from(accounts)?;

        Ok(Self { accounts })
    }
}

impl<'a> Repay<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;
    
    pub fn process(&mut self) -> ProgramResult {  
        let loan_data = self.accounts.loan.try_borrow_data()?;
        let loan_num = loan_data.len() / size_of::<LoanData>();

        if loan_num != self.accounts.token_accounts.len() {
            return Err(ProgramError::InvalidAccountData);
        }

        // Process each pair of token accounts (protocol, borrower) with corresponding amounts
        for i in 0..loan_num {
            // Validate that protocol_ata is the same as the one in the loan account
            let protocol_token_account = &self.accounts.token_accounts[i];

            if unsafe { *(loan_data.as_ptr().add(i * size_of::<LoanData>()) as *const [u8; 32]) } != *protocol_token_account.key() {
                return Err(ProgramError::InvalidAccountData);
            }

            // Check if the loan is already repaid
            let balance = get_token_amount(&protocol_token_account.try_borrow_data()?);
            let loan_balance = unsafe { *(loan_data.as_ptr().add(i * size_of::<LoanData>() + size_of::<[u8; 32]>()) as *const u64) };

            if balance < loan_balance {
                return Err(ProgramError::InvalidAccountData);
            }
        }
        
        Ok(())
    }
}