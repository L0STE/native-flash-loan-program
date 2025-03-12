use pinocchio::instruction::Signer;
use pinocchio::pubkey::create_program_address;
use pinocchio::sysvars::clock::Clock;
use pinocchio::sysvars::Sysvar;
use pinocchio::{account_info::AccountInfo, instruction::Seed, program_error::ProgramError, ProgramResult};
use constant_product_curve::{ConstantProduct, LiquidityPair};
use pinocchio_token::instructions::Transfer;
use crate::instructions::helpers::{SignerAccount, ProgramAccount, MintAccount, AssociatedTokenAccount, TokenAccount};
use crate::state::Config;
use core::mem::size_of;

/// #Swap
/// 
/// Swap from Token X to Token Y or vice versa
/// 
/// Accounts:
/// 
/// 1. user:                        [signer, mut]
/// 2. mint_x:                      [mut]
/// 3. mint_y:                      [mut]
/// 4. user_x:                      [init_if_needed]
/// 5. user_y:                      [init_if_needed]
/// 7. vault_x                      [mut]
/// 8. vault_y                      [mut]
/// 9. config                       
/// 10. system_program              [executable]
/// 11. token_program               [executable]
/// 12. associated_token_program    [executable]
/// 
/// Parameters:
/// 
/// 1. is_x:                        [bool]
/// 2. amount:                      [u64]
/// 3. min:                         [u64]
/// 4. expiration:                  [u64]
pub struct SwapAccounts<'a> {
    pub user: &'a AccountInfo,
    pub mint_x: &'a AccountInfo,
    pub mint_y: &'a AccountInfo,
    pub user_x: &'a AccountInfo,
    pub user_y: &'a AccountInfo,
    pub vault_x: &'a AccountInfo,
    pub vault_y: &'a AccountInfo,
    pub config: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for SwapAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [user, mint_x, mint_y, user_x, user_y, vault_x, vault_y, config, system_program, token_program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Accounts Checks
        SignerAccount::check(user)?;
        MintAccount::check(mint_x)?;
        MintAccount::check(mint_y)?;
        AssociatedTokenAccount::check(vault_x, config, user)?;
        AssociatedTokenAccount::check(vault_y, config, user)?;
        ProgramAccount::check(config)?;

        
        // Return the accounts
        Ok(Self {
            user,
            mint_x,
            mint_y,
            user_x,
            user_y,
            vault_x,
            vault_y,
            config,
            system_program,
            token_program,
        })
    }
}

pub struct SwapInstructionData {
    pub is_x: bool,
    pub amount: u64,
    pub min: u64,
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for SwapInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<u16>() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let is_x = data[0];

        let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
        if amount == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let min = u64::from_le_bytes(data[9..17].try_into().unwrap());
        if min == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let expiration = i64::from_le_bytes(data[17..25].try_into().unwrap());
        if expiration < Clock::get()?.unix_timestamp {
            return Err(ProgramError::InvalidInstructionData);
        }

        match is_x {
            0 => Ok(Self { is_x: false, amount, min, expiration }),
            1 => Ok(Self { is_x: true, amount, min, expiration }),
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }
}

pub struct Swap<'a> {
    pub accounts: SwapAccounts<'a>,
    pub instruction_data: SwapInstructionData,
    pub amounts: (u64, u64),
    pub seed: [u8; 8],
    pub bump: [u8; 1],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Swap<'a> {
    type Error = ProgramError;
    
    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = SwapAccounts::try_from(accounts)?;
        let instruction_data = SwapInstructionData::try_from(data)?;

        // Create ATAs if needed
        if instruction_data.is_x {
            AssociatedTokenAccount::init(
                accounts.user_y,
                accounts.mint_y,
                accounts.user,
                accounts.user,
                accounts.system_program,
                accounts.token_program,
            )?;
        } else {
            AssociatedTokenAccount::init(
                accounts.user_x,
                accounts.mint_x,
                accounts.user,
                accounts.user,
                accounts.system_program,
                accounts.token_program,
            )?;
        }

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

        // Swap Calculations
        let mut curve = ConstantProduct::init(
            TokenAccount::get_amount(accounts.vault_x),
            TokenAccount::get_amount(accounts.vault_y),
            TokenAccount::get_amount(accounts.vault_x),
            config.fee,
            None
        ).map_err(|_| ProgramError::Custom(1))?;

        let p = match instruction_data.is_x {
            true => LiquidityPair::X,
            false => LiquidityPair::Y
        };

        let swap_result= curve.swap(p, instruction_data.amount, instruction_data.min).map_err(|_| ProgramError::Custom(1))?;

        // Check for correct values
        if swap_result.deposit == 0 || swap_result.withdraw == 0 {
            return Err(ProgramError::InvalidArgument);
        }

        // Addresses Check
        let config_key = create_program_address(&[b"config", &config.seed.to_le_bytes(), accounts.mint_x.key().as_ref(), accounts.mint_y.key().as_ref(), &[config.config_bump]], &crate::ID)?;
        if &config_key != accounts.config.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
            amounts: (swap_result.deposit, swap_result.withdraw),
            seed: config.seed.to_le_bytes(),
            bump: [config.config_bump],
        })
    }
}
impl<'a> Swap<'a> {
    pub const DISCRIMINATOR: &'a u8 = &3;
    
    pub fn process(&mut self) -> ProgramResult {

        let seeds = [
            Seed::from("config".as_bytes()),
            Seed::from(&self.seed),
            Seed::from(self.accounts.mint_x.key().as_ref()),
            Seed::from(self.accounts.mint_y.key().as_ref()),
            Seed::from(&self.bump),
        ];
        let signer_seeds = [Signer::from(&seeds)];
        
        if self.instruction_data.is_x {
            Transfer {
                from: self.accounts.user_x,
                to: self.accounts.vault_x,
                authority: self.accounts.user,
                amount: self.amounts.0,
            }.invoke()?;
            
            Transfer {
                from: self.accounts.vault_y,
                to: self.accounts.user_y,
                authority: self.accounts.config,
                amount: self.amounts.1,
            }.invoke_signed(&signer_seeds)?;
        } else {
            Transfer {
                from: self.accounts.user_y,
                to: self.accounts.vault_y,
                authority: self.accounts.user,
                amount: self.amounts.0,
            }.invoke()?;
        
            Transfer {
                from: self.accounts.vault_x,
                to: self.accounts.user_x,
                authority: self.accounts.config,
                amount: self.amounts.1,
            }.invoke_signed(&signer_seeds)?;
        }
        
        Ok(())
    }
}
