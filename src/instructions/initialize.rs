use pinocchio::pubkey::find_program_address;
use pinocchio::{account_info::AccountInfo, instruction::Seed, program_error::ProgramError, ProgramResult};
use crate::instructions::helpers::{SignerAccount, ProgramAccount, MintAccount, AssociatedTokenAccount};
use crate::state::Config;
use core::mem::size_of;

/// #Initialize
/// 
/// Initialize the Amm
/// 
/// Accounts:
/// 
/// 1. initializer:                 [signer, mut]
/// 2. mint_x:                      [mut]
/// 3. mint_y:                      [mut]
/// 4. mint_lp:                     [init]
/// 5. vault_x                      [init]
/// 6. vault_y                      [init]
/// 7. authority                    
/// 8. config                       [init]
/// 9. system_program               [executable]
/// 10. token_program                [executable]
/// 11. associated_token_program     [executable]
/// 
/// Parameters:
/// 
/// 1. seed:          [u64]
/// 2. fee:           [u16]
/// 3. authority:     [Option<Pubkey>]
pub struct InitializeAccounts<'a> {
    pub initializer: &'a AccountInfo,
    pub mint_x: &'a AccountInfo,
    pub mint_y: &'a AccountInfo,
    pub mint_lp: &'a AccountInfo,
    pub vault_x: &'a AccountInfo,
    pub vault_y: &'a AccountInfo,
    pub authority: &'a AccountInfo,
    pub config: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [initializer, mint_x, mint_y, mint_lp, vault_x, vault_y, authority, config, system_program, token_program, _] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Accounts Checks
        SignerAccount::check(initializer)?;
        MintAccount::check(mint_x)?;
        MintAccount::check(mint_y)?;

        
        // Return the accounts
        Ok(Self {
            initializer,
            mint_x,
            mint_y,
            mint_lp,
            vault_x,
            vault_y,
            authority,
            config,
            system_program,
            token_program,
        })
    }
}

pub struct InitializeInstructionData {
    pub seed: u64,
    pub fee: u16,
    pub authority: Option<[u8; 32]>,
}

impl<'a> TryFrom<&'a [u8]> for InitializeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        match data.len() {
            len if len == size_of::<u64>() + size_of::<u16>() => {
                let seed = u64::from_le_bytes(data[0..8].try_into().unwrap());
                let fee = u16::from_le_bytes(data[8..10].try_into().unwrap());
                if fee >= 10000 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                Ok(Self { seed, fee, authority: None })
            },
            len if len == size_of::<u64>() + size_of::<u16>() + size_of::<[u8; 32]>() => {
                let seed = u64::from_le_bytes(data[0..8].try_into().unwrap());
                let fee = u16::from_le_bytes(data[8..10].try_into().unwrap());
                if fee >= 10000 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                let authority = data[10..42].try_into().unwrap();
                Ok(Self { seed, fee, authority: Some(authority) })
            },
            _ => return Err(ProgramError::InvalidInstructionData),
        }
    }
}

pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
    pub instruction_data: InitializeInstructionData,
    pub bump: [u8; 3],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Initialize<'a> {
    type Error = ProgramError;
    
    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = InitializeAccounts::try_from(accounts)?;
        let instruction_data = InitializeInstructionData::try_from(data)?;
        
        // Initialize the Accounts needed
        let (config_key, config_bump) = find_program_address(&[b"config", &instruction_data.seed.to_le_bytes(), accounts.mint_x.key().as_ref(), accounts.mint_y.key().as_ref()], &crate::ID);
        if &config_key != accounts.config.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        let seed_binding = instruction_data.seed.to_le_bytes();
        let config_bump_binding = [config_bump];
        let config_seeds = [
            Seed::from(b"config"),
            Seed::from(&seed_binding),
            Seed::from(&config_bump_binding),
        ];

        ProgramAccount::init::<Config>(
            accounts.initializer,
            accounts.config,
            &config_seeds,
            Config::LEN
        )?;

        let (lp_key, lp_bump) = find_program_address(&[b"lp"], &crate::ID);
        if &lp_key != accounts.mint_lp.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        let lp_bump_binding = [lp_bump];
        let lp_seeds = [
            Seed::from(b"lp"),
            Seed::from(&lp_bump_binding),
        ];

        MintAccount::init_with_seeds(
            accounts.mint_lp,
            accounts.initializer,
            6,
            accounts.authority.key(),
            None,
            &lp_seeds,
        )?;

        AssociatedTokenAccount::init(
            accounts.vault_x,
            accounts.mint_x,
            accounts.authority,
            accounts.config,
            accounts.system_program,
            accounts.token_program,
        )?;

        AssociatedTokenAccount::init(
            accounts.vault_y,
            accounts.mint_y,
            accounts.authority,
            accounts.config,
            accounts.system_program,
            accounts.token_program,
        )?;

        // Get the auth bump
        let (auth_key, auth_bump) = find_program_address(&[b"auth"], &crate::ID);
        if &auth_key != accounts.authority.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
            bump: [config_bump, lp_bump, auth_bump],
        })
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;
    
    pub fn process(&mut self) -> ProgramResult {
        let mut data = self.accounts.config.try_borrow_mut_data()?;
        let config = Config::load_mut(data.as_mut())?;
        
        config.set_inner(
            self.instruction_data.seed, 
            if let Some(authority) = self.instruction_data.authority {
                authority
            } else {
                pinocchio_system::ID
            }, 
            *self.accounts.mint_x.key(), 
            *self.accounts.mint_y.key(), 
            self.instruction_data.fee, 
            false, 
            self.bump[0], 
            self.bump[1], 
            self.bump[2]
        );
        
        Ok(())
    }
}