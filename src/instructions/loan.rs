use pinocchio::instruction::{Seed, Signer};
use pinocchio::pubkey::try_find_program_address;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use pinocchio_token::instructions::Transfer;
use core::mem::size_of;
use core::mem::MaybeUninit;

use crate::{LoanData, ID, MAX_LOAN_PAIRS};

/// #Loan
/// 
/// Loan tokens from the protocol
/// 
/// Accounts:
/// 
/// 1. borrower:                        [signer, mut]
/// 2. protocol:                    
/// 3. instruction_sysvar:
/// 4. token_program                    [executable]
/// ..remaining accounts are token accounts from protocol and borrower
/// Parameters:
/// 
/// [1..]. amount: u64,                 // Amount of token to loan
pub struct LoanAccounts<'a> {
    pub borrower: &'a AccountInfo,
    pub protocol: &'a AccountInfo,
    pub loan_data: &'a [LoanData<'a>],
    pub instruction_sysvar: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for LoanAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [borrower, protocol, instruction_sysvar, token_program, rest @ ..] = accounts else {
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
            protocol,
            loan_data,
            instruction_sysvar,
            token_program,
        })
    }
}

pub struct LoanInstructionData<'a> {
    pub amounts: &'a [u64],
}

impl<'a> TryFrom<&'a [u8]> for LoanInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() % size_of::<u64>() != 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let mut amounts_data_array: [MaybeUninit<u64>; MAX_LOAN_PAIRS] = [MaybeUninit::uninit(); MAX_LOAN_PAIRS];

        for (i, chunk) in data.chunks(size_of::<u64>()).enumerate() {
            amounts_data_array[i] = MaybeUninit::new(u64::from_le_bytes(chunk.try_into().unwrap()));
        }

        let amounts = unsafe {
            core::slice::from_raw_parts(
                amounts_data_array.as_ptr() as *const u64,
                data.len()
            )
        };

        Ok(Self { amounts })
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

        if instruction_data.amounts.len() != accounts.loan_data.len() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let (_, bump) = try_find_program_address(&[b"auth"], &ID).ok_or(ProgramError::InvalidAccountData)?;

        // Instruction Sysvar to do

        // Return the initialized struct
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
        let signer_seeds = [
            Seed::from("auth".as_bytes()),
            Seed::from(&self.bump),
        ];
        let signer_seeds = [Signer::from(&signer_seeds)];

        for (loan_data, amount) in self.accounts.loan_data.iter().zip(self.instruction_data.amounts.iter()) {
            Transfer {
                from: loan_data.protocol_token_accounts,
                to: loan_data.borrower_token_accounts,
                authority: self.accounts.protocol,
                amount: *amount,
            }.invoke_signed(&signer_seeds)?;
        }
        
        Ok(())
    }
}