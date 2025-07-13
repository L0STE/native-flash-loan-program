use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use pinocchio_token::instructions::Transfer;
use crate::{get_token_amount, LoanData, MAX_LOAN_PAIRS};

/// #Repay
/// 
/// Repay tokens to the protocol
/// 
/// Accounts:
/// 
/// 1. borrower:                        [signer, mut]
/// 2. loan:
/// 3. token_program:
/// 4. ..remaining accounts are token accounts from protocol and borrower
pub struct RepayAccounts<'a> {
    pub borrower: &'a AccountInfo,
    pub loan: &'a AccountInfo,
    pub token_accounts: &'a [AccountInfo],
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for RepayAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [borrower, loan, token_program, rest @ ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if rest.len() % 2 != 0 || rest.len() / 2 > MAX_LOAN_PAIRS || rest.len() < 2 {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Self {
            borrower,
            loan,
            token_accounts: rest,
            token_program,
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

        // Process each pair of token accounts (protocol, borrower) with corresponding amounts
        for i in 0..loan_num {
            let protocol_token_account = &self.accounts.token_accounts[i * 2];
            let borrower_token_account = &self.accounts.token_accounts[i * 2 + 1];

            // Validate that we're repaying the correct protocol_ata
            if unsafe { *(loan_data.as_ptr().add(i * size_of::<LoanData>()) as *const [u8; 32]) } != *protocol_token_account.key() {
                return Err(ProgramError::InvalidAccountData);
            }

            // Check if we already repaid this loan, if not do it with a simple transfer.
            let balance = get_token_amount(&borrower_token_account.try_borrow_data()?);
            let loan_balance = unsafe { *(loan_data.as_ptr().add(i * size_of::<LoanData>() + size_of::<[u8; 32]>()) as *const u64) };

            if balance < loan_balance {
                let amount = loan_balance - balance;
    
                Transfer {
                    from: borrower_token_account,
                    to: protocol_token_account,
                    authority: self.accounts.borrower,
                    amount,
                }.invoke()?;
            }
        }
        
        Ok(())
    }
}