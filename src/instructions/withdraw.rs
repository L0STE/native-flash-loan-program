use pinocchio::instruction::{Seed, Signer};
use pinocchio::sysvars::clock::Clock;
use pinocchio::sysvars::Sysvar;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use constant_product_curve::ConstantProduct;
use pinocchio_token::instructions::{Burn, Transfer};
use crate::instructions::helpers::{SignerAccount, TokenAccount, MintAccount, AssociatedTokenAccount};
use crate::state::Config;
use core::mem::size_of;

/// #Withdraw
/// 
/// Withdraw tokens from the Amm
/// 
/// Accounts:
/// 
/// 1. user:                         [signer, mut]
/// 2. mint_x:                       
/// 3. mint_y:      
/// 4. mint_lp                      [mut]
/// 5. vault_x                      [mut]
/// 6. vault_y                      [mut]
/// 7. user_x_ata                   [init_if_needed]
/// 8. user_y_ata                   [init_if_needed]
/// 9. user_lp_ata                  [mut]
/// 10. authority                    
/// 11. config                       
/// 12. system_program               [executable]
/// 13. token_program                [executable]
/// 14. associated_token_program     [executable]
/// 
/// Parameters:
/// 
/// 1. amount: u64,        // Amount of LP token to claim
/// 2. min_x: u64,         // Min amount of X we are willing to receive
/// 3. min_y: u64,         // Min amount of Y we are willing to receive
/// 4. expiration: i64     // Expiration of the offer
pub struct WithdrawAccounts<'a> {
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

impl<'a> TryFrom<&'a [AccountInfo]> for WithdrawAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [user, mint_x, mint_y, mint_lp, vault_x, vault_y, user_x_ata, user_y_ata, user_lp_ata, authority, config, system_program, token_program, _] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Accounts Checks
        SignerAccount::check(user)?;
        AssociatedTokenAccount::check(vault_x, config, mint_x)?;
        AssociatedTokenAccount::check(vault_y, config, mint_y)?;
        AssociatedTokenAccount::check(user_lp_ata, user, mint_lp)?;

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

pub struct WithdrawInstructionData {
    pub amount: u64,
    pub min_x: u64,
    pub min_y: u64,
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for WithdrawInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<u64>() + size_of::<u64>() + size_of::<u64>() + size_of::<i64>() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
        if amount == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let min_x = u64::from_le_bytes(data[8..16].try_into().unwrap());
        if min_x == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let min_y = u64::from_le_bytes(data[16..24].try_into().unwrap());
        if min_y == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let expiration = i64::from_le_bytes(data[24..32].try_into().unwrap());
        if expiration < Clock::get()?.unix_timestamp {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self { amount, min_x, min_y, expiration })
    }
}

pub struct Withdraw<'a> {
    pub accounts: WithdrawAccounts<'a>,
    pub instruction_data: WithdrawInstructionData,
    pub amounts: (u64, u64),
    pub seed: [u8; 8],
    pub bump: [u8; 1],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Withdraw<'a> {
    type Error = ProgramError;
    
    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = WithdrawAccounts::try_from(accounts)?;
        let instruction_data = WithdrawInstructionData::try_from(data)?;
        
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

        let (x, y) = match MintAccount::supply(accounts.mint_lp) == instruction_data.amount {
            true => (TokenAccount::get_amount(accounts.vault_x), TokenAccount::get_amount(accounts.vault_y)),
            false => {
                let amounts = ConstantProduct::xy_withdraw_amounts_from_l(
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
        if !(x <= instruction_data.min_x && y <= instruction_data.min_y) {
            return Err(ProgramError::InvalidArgument);
        }

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
            amounts: (x, y),
            seed: config.seed.to_le_bytes(),
            bump: [config.auth_bump],
        })
    }
}

impl<'a> Withdraw<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;
    
    pub fn process(&mut self) -> ProgramResult {

        let seeds = [
            Seed::from("config".as_bytes()),
            Seed::from(&self.seed),
            Seed::from(self.accounts.mint_x.key().as_ref()),
            Seed::from(self.accounts.mint_y.key().as_ref()),
            Seed::from(&self.bump),
        ];
        let signer_seeds = [Signer::from(&seeds)];
        
        Transfer {
            from: self.accounts.vault_x,
            to: self.accounts.user_x_ata,
            authority: self.accounts.config,
            amount: self.amounts.0,
        }.invoke_signed(&signer_seeds)?;

        Transfer {
            from: self.accounts.vault_y,
            to: self.accounts.user_y_ata,
            authority: self.accounts.config,
            amount: self.amounts.1,
        }.invoke_signed(&signer_seeds)?;

        Burn {
            mint: self.accounts.mint_lp,
            account: self.accounts.user_lp_ata,
            authority: self.accounts.user,
            amount: self.instruction_data.amount,
        }.invoke()?;
        
        Ok(())
    }
}