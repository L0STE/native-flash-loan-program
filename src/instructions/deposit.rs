use pinocchio::instruction::{Seed, Signer};
use pinocchio::sysvars::clock::Clock;
use pinocchio::sysvars::Sysvar;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use constant_product_curve::ConstantProduct;
use pinocchio_token::instructions::{MintTo, Transfer};
use crate::instructions::helpers::{SignerAccount, TokenAccount, MintAccount, AssociatedTokenAccount};
use crate::state::Config;
use core::mem::size_of;

/// #Deposit
/// 
/// Deposit tokens into the Amm
/// 
/// Accounts:
/// 
/// 1. user:                         [signer, mut]
/// 2. mint_x:                       
/// 3. mint_y:      
/// 4. mint_lp                      [mut]
/// 5. vault_x                      [mut]
/// 6. vault_y                      [mut]
/// 7. user_x_ata                   [mut]
/// 8. user_y_ata                   [mut]
/// 9. user_lp_ata                  [init_if_needed]
/// 10. authority                    
/// 11. config                       
/// 12. system_program               [executable]
/// 13. token_program                [executable]
/// 14. associated_token_program     [executable]
/// 
/// Parameters:
/// 
/// 1. amount: u64,        // Amount of LP token to claim
/// 2. max_x: u64,         // Max amount of X we are willing to deposit
/// 3. max_y: u64,         // Max amount of Y we are willing to deposit
/// 4. expiration: i64     // Expiration of the offer
pub struct DepositAccounts<'a> {
    pub user: &'a AccountInfo,
    pub mint_x: &'a AccountInfo,
    pub mint_y: &'a AccountInfo,
    pub mint_lp: &'a AccountInfo,
    pub vault_x: &'a AccountInfo,
    pub vault_y: &'a AccountInfo,
    pub user_x_ata: &'a AccountInfo,
    pub user_y_ata: &'a AccountInfo,
    pub user_lp_ata: &'a AccountInfo,
    pub authority: &'a AccountInfo,
    pub config: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for DepositAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [user, mint_x, mint_y, mint_lp, vault_x, vault_y, user_x_ata, user_y_ata, user_lp_ata, authority, config, system_program, token_program, _] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Accounts Checks
        SignerAccount::check(user)?;
        AssociatedTokenAccount::check(vault_x, config, mint_x)?;
        AssociatedTokenAccount::check(vault_y, config, mint_y)?;
        AssociatedTokenAccount::check(user_x_ata, user, mint_x)?;
        AssociatedTokenAccount::check(user_y_ata, user, mint_y)?;
        
        // Return the accounts
        Ok(Self {
            user,
            mint_x,
            mint_y,
            mint_lp,
            vault_x,
            vault_y,
            user_x_ata,
            user_y_ata,
            user_lp_ata,
            authority,
            config,
            system_program,
            token_program,
        })
    }
}

pub struct DepositInstructionData {
    pub amount: u64,
    pub max_x: u64,
    pub max_y: u64,
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for DepositInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<u64>() + size_of::<u64>() + size_of::<u64>() + size_of::<i64>() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
        if amount == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let max_x = u64::from_le_bytes(data[8..16].try_into().unwrap());
        if max_x == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let max_y = u64::from_le_bytes(data[16..24].try_into().unwrap());
        if max_y == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let expiration = i64::from_le_bytes(data[24..32].try_into().unwrap());
        if expiration < Clock::get()?.unix_timestamp {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self { amount, max_x, max_y, expiration })
    }
}

pub struct Deposit<'a> {
    pub accounts: DepositAccounts<'a>,
    pub instruction_data: DepositInstructionData,
    pub amounts: (u64, u64),
    pub bump: [u8; 1],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Deposit<'a> {
    type Error = ProgramError;
    
    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = DepositAccounts::try_from(accounts)?;
        let instruction_data = DepositInstructionData::try_from(data)?;

        // Initialize Accounts
        AssociatedTokenAccount::init_if_needed(accounts.user_lp_ata, accounts.mint_lp, accounts.user, accounts.user, accounts.system_program, accounts.token_program)?;
        
        // Config Account Checks
        let mut data = accounts.config.try_borrow_mut_data()?;
        let config = Config::load_mut(data.as_mut())?;

        if config.is_locked() {
            return Err(ProgramError::UninitializedAccount);
        }

        if config.mint_x != *accounts.mint_x.key() {
            return Err(ProgramError::InvalidArgument);
        }

        if config.mint_y != *accounts.mint_y.key() {
            return Err(ProgramError::InvalidArgument);
        }

        let (x,y) = match MintAccount::supply(accounts.mint_lp) == 0 && TokenAccount::get_amount(accounts.vault_x) == 0 && TokenAccount::get_amount(accounts.vault_y) == 0 {
            true => (instruction_data.max_x, instruction_data.max_y),
            false => {
                let amounts = ConstantProduct::xy_deposit_amounts_from_l(
                    TokenAccount::get_amount(accounts.vault_x),
                    TokenAccount::get_amount(accounts.vault_y),
                    MintAccount::supply(accounts.mint_lp),
                    instruction_data.amount,
                    6
                ).map_err(|_| ProgramError::InvalidArgument)?;                
                
                (amounts.x, amounts.y)
            }
        };

        // Check for slippage
        if !(x <= instruction_data.max_x && y <= instruction_data.max_y) {
            return Err(ProgramError::InvalidArgument);
        }

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
            amounts: (x, y),
            bump: [config.auth_bump],
        })
    }
}

impl<'a> Deposit<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;
    
    pub fn process(&mut self) -> ProgramResult {

        Transfer {
            from: self.accounts.user_x_ata,
            to: self.accounts.vault_x,
            authority: self.accounts.user,
            amount: self.amounts.0,
        }.invoke()?;

        Transfer {
            from: self.accounts.user_y_ata,
            to: self.accounts.vault_y,
            authority: self.accounts.user,
            amount: self.amounts.1,
        }.invoke()?;

        let seeds = [
            Seed::from("auth".as_bytes()),
            Seed::from(&self.bump),
        ];
        let signer_seeds = [Signer::from(&seeds)];
        MintTo {
            mint: self.accounts.mint_lp,
            account: self.accounts.user_lp_ata,
            mint_authority: self.accounts.authority,
            amount: self.instruction_data.amount,
        }.invoke_signed(&signer_seeds)?;
        
        
        Ok(())
    }
}