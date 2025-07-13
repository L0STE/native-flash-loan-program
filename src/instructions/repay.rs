use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult, sysvars::instructions::Instructions};
use pinocchio_token::instructions::Transfer;
use core::mem::MaybeUninit;
use crate::{LoanData, FEE, MAX_LOAN_PAIRS};

/// #Repay
/// 
/// Repay tokens to the protocol
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

        if rest.len() % 2 != 0 || rest.len() / 2 > MAX_LOAN_PAIRS || rest.len() < 2 {
            return Err(ProgramError::InvalidAccountData);
        }

        let mut loan_data_array: [MaybeUninit<LoanData<'a>>; MAX_LOAN_PAIRS] = [MaybeUninit::uninit(); MAX_LOAN_PAIRS];

        for (i, chunk) in rest.chunks(2).enumerate() {
            loan_data_array[i] = MaybeUninit::new(LoanData {
                protocol_token_accounts: &chunk[0],
                borrower_token_accounts: &chunk[1],
            });
        }
        
        let loan_data = unsafe {
            core::slice::from_raw_parts(
                loan_data_array.as_ptr() as *const LoanData<'a>,
                rest.len() / 2
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

        if self.accounts.loan_data.len() * 2 != unsafe { *(instruction.raw as *const u16) as usize - 4} {
            return Err(ProgramError::InvalidAccountData);
        }

        let instruction_data = instruction.get_instruction_data();

        let amounts: &[u64] = unsafe {
            core::slice::from_raw_parts(
                instruction_data.as_ptr() as *const u64,
                instruction_data.len() / 8
            )
        };
        
        for (i, (loan_data, &amount)) in self.accounts.loan_data.iter().zip(amounts.iter()).enumerate() {  
            let protocol_key = unsafe { instruction.get_account_meta_at_unchecked(4 + (i * 2)).key };
            let borrower_key = unsafe { instruction.get_account_meta_at_unchecked(5 + (i * 2)).key };
            
            if loan_data.protocol_token_accounts.key() != &protocol_key ||
                loan_data.borrower_token_accounts.key() != &borrower_key {
                    return Err(ProgramError::InvalidAccountData);
                }

            let fee = (amount as u128)
                .checked_mul(FEE)
                .and_then(|x| x.checked_div(10_000))
                .ok_or(ProgramError::InvalidAccountData)? as u64;
            
            Transfer {
                from: loan_data.borrower_token_accounts,
                to: loan_data.protocol_token_accounts,
                authority: self.accounts.borrower,
                amount: amount + fee,
            }.invoke()?;
        }
        
        Ok(())
    }
}