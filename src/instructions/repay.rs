use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult, sysvars::instructions::Instructions};
use pinocchio_token::instructions::Transfer;
use core::mem::MaybeUninit;
use crate::{LoanData, FEE, MAX_LOAN_PAIRS};

/// #Loan
/// 
/// Loan tokens from the protocol
/// 
/// Accounts:
/// 
/// 1. borrower:                        [signer, mut]
/// 2. instruction_sysvar:
/// 3. token_program:
/// 4. ..remaining accounts are token accounts from protocol and borrower
pub struct RepayAccounts<'a> {
    pub borrower: &'a AccountInfo,
    pub loan_data: &'a [LoanData<'a>],
    pub instruction_sysvar: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for RepayAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [borrower, instruction_sysvar, token_program, rest @ ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if rest.len() % 2 != 0 || rest.len() / 2 > MAX_LOAN_PAIRS || rest.len() > 2 {
            return Err(ProgramError::InvalidAccountData);
        }

        let mut loan_data_array: [MaybeUninit<LoanData<'a>>; MAX_LOAN_PAIRS] = [MaybeUninit::uninit(); MAX_LOAN_PAIRS];
        
        for (i, chunk) in rest.chunks(2).enumerate() {
            loan_data_array[i] = MaybeUninit::new(LoanData {
                protocol_token_accounts: &chunk[0],
                borrower_token_accounts: &chunk[1],
            });
        }
        
        // Convert the MaybeUninit array to initialized slice
        let loan_data = unsafe {
            core::slice::from_raw_parts(
                loan_data_array.as_ptr() as *const LoanData<'a>,
                rest.len()
            )
        };

        Ok(Self {
            borrower,
            loan_data,
            instruction_sysvar,
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
        let instruction_sysvar = unsafe { Instructions::new_unchecked(self.accounts.instruction_sysvar.try_borrow_data()?) };        
        let instruction = instruction_sysvar.load_instruction_at(0)?;

        if instruction.get_program_id() != &crate::ID {
            return Err(ProgramError::InvalidInstructionData);
        }

        if self.accounts.loan_data.len() * 2 != u16::from_le_bytes(unsafe { *(instruction.raw as *const [u8; 2]) }) as usize - 4{
            return Err(ProgramError::InvalidAccountData);
        }

        let instruction_data = instruction.get_instruction_data();

        for (i, loan_data) in self.accounts.loan_data.iter().enumerate() {
            if loan_data.protocol_token_accounts.key() != &unsafe { instruction.get_account_meta_at_unchecked(i + 5).key } {
                return Err(ProgramError::InvalidAccountData);
            }

            if loan_data.borrower_token_accounts.key() != &unsafe { instruction.get_account_meta_at_unchecked(i + 5 + 1).key } {
                return Err(ProgramError::InvalidAccountData);
            }

            let mut amount = u64::from_le_bytes(unsafe { *(instruction_data.as_ptr().add(i * 8) as *const [u8; 8]) });
            amount = (amount as u128).checked_mul(FEE).ok_or(ProgramError::InvalidAccountData)?.checked_div(10_000).ok_or(ProgramError::InvalidAccountData)? as u64;

            Transfer {
                from: loan_data.borrower_token_accounts,
                to: loan_data.protocol_token_accounts,
                authority: self.accounts.borrower,
                amount,
            }.invoke()?;
        }
        
        Ok(())
    }
}