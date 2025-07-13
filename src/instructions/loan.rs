use pinocchio::instruction::{Seed, Signer};
use pinocchio::pubkey::try_find_program_address;
use pinocchio::sysvars::instructions::Instructions;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use pinocchio_token::instructions::Transfer;
use core::mem::size_of;
use crate::{get_token_amount, LoanData, Repay, ID, MAX_LOAN_PAIRS};

/// #Loan
/// 
/// Loan tokens from the protocol
/// 
/// Accounts:
/// 
/// 1. borrower:                        [signer, mut]
/// 2. protocol:                    
/// 3. loan:                       
/// 4. instruction_sysvar:
/// 5. token_program                    [executable]
/// ..remaining accounts are token accounts from protocol and borrower
/// Parameters:
/// 1. fee: u64,                        // Fee to pay to the protocol
/// 2..n. amount: u64,                  // Amount of token to loan
pub struct LoanAccounts<'a> {
    pub borrower: &'a AccountInfo,
    pub protocol: &'a AccountInfo,
    pub loan: &'a AccountInfo,
    pub token_accounts: &'a [AccountInfo],
    pub instruction_sysvar: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for LoanAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [borrower, protocol, loan, instruction_sysvar, token_program, rest @ ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Verify that the number of token accounts is valid
        if rest.len() % 2 != 0 || rest.len() / 2 > MAX_LOAN_PAIRS || rest.len() < 2 {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Self {
            borrower,
            protocol,
            loan,
            token_accounts: rest,
            instruction_sysvar,
            token_program,
        })
    }
}

pub struct LoanInstructionData<'a> {
    pub fee: u16,
    pub amounts: &'a [u64],
}

impl<'a> TryFrom<&'a [u8]> for LoanInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        // Verify that the data is valid
        if data.len() < size_of::<u16>() + size_of::<u64>() || (data.len() - size_of::<u16>()) % size_of::<u64>() != 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        // Get the fee
        let fee = u16::from_le_bytes(unsafe { *(data.as_ptr() as *const [u8; 2]) });

        // Get the amounts
        let amounts: &[u64] = unsafe {
            core::slice::from_raw_parts(
                data.as_ptr().add(size_of::<u64>()) as *const u64,
                data.len() / size_of::<u64>()
            )
        };

        Ok(Self { fee, amounts })
    }
}

pub struct Loan<'a> {
    pub accounts: LoanAccounts<'a>,
    pub instruction_data: LoanInstructionData<'a>,
    pub bump: [u8; 1],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Loan<'a> {
    type Error = ProgramError;
    
    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = LoanAccounts::try_from(accounts)?;
        let instruction_data = LoanInstructionData::try_from(data)?;

        // Verify that the number of amounts matches the number of token accounts
        if instruction_data.amounts.len() != accounts.token_accounts.len() / 2 {
            return Err(ProgramError::InvalidInstructionData);
        }

        // Get the bump for the protocol account
        let (_, bump) = try_find_program_address(&[b"protocol", &instruction_data.fee.to_le_bytes()], &ID).ok_or(ProgramError::InvalidAccountData)?;

        Ok(Self {
            accounts,
            instruction_data,
            bump: [bump],
        })
    }
}

impl<'a> Loan<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;
    
    pub fn process(&mut self) -> ProgramResult {
        // Get the fee
        let fee = self.instruction_data.fee.to_le_bytes();

        // Get the signer seeds
        let signer_seeds = [
            Seed::from("protocol".as_bytes()),
            Seed::from(&fee),
            Seed::from(&self.bump),
        ];
        let signer_seeds = [Signer::from(&signer_seeds)];

        // Get the loan account as mutable so we can push the Loan struct to it
        let loan_data = self.accounts.loan.try_borrow_mut_data()?.as_ptr();

        for (i, amount) in self.instruction_data.amounts.iter().enumerate() {
            let protocol_token_account = &self.accounts.token_accounts[i * 2];
            let borrower_token_account = &self.accounts.token_accounts[i * 2 + 1];

            // Get the balance of the borrower's token account and add the fee to it so we can save it to the loan account
            let balance = get_token_amount(&borrower_token_account.try_borrow_data()?);
            let balance_with_fee = balance.checked_add(
                amount.checked_mul(self.instruction_data.fee as u64)
                    .and_then(|x| x.checked_div(10_000))
                    .ok_or(ProgramError::InvalidInstructionData)?
            ).ok_or(ProgramError::InvalidInstructionData)?;

            // Push the Loan struct to the loan account
            unsafe {
                *(loan_data.add(i * size_of::<LoanData>()) as *mut LoanData) = LoanData {
                    protocol_token_accounts: protocol_token_account,
                    balance: balance_with_fee,
                }
            }

            // Transfer the tokens from the protocol to the borrower
            Transfer {
                from: protocol_token_account,
                to: borrower_token_account,
                authority: self.accounts.protocol,
                amount: *amount,
            }.invoke_signed(&signer_seeds)?;
        }

        // Introspecting the Repay instruction
        let num_instructions = unsafe { *(self.accounts.instruction_sysvar.try_borrow_data()?.as_ptr() as *const u16) };

        let instruction_sysvar = unsafe { Instructions::new_unchecked(self.accounts.instruction_sysvar.try_borrow_data()?) };        
        let instruction = instruction_sysvar.load_instruction_at(num_instructions as usize - 1)?;

        if instruction.get_program_id() != &crate::ID {
            return Err(ProgramError::InvalidInstructionData);
        }

        if unsafe { *(instruction.get_instruction_data().as_ptr()) } != *Repay::DISCRIMINATOR {
            return Err(ProgramError::InvalidInstructionData);
        }

        if unsafe { instruction.get_account_meta_at_unchecked(1).key } != *self.accounts.loan.key() {
            return Err(ProgramError::InvalidInstructionData);
        }
        
        Ok(())
    }
}