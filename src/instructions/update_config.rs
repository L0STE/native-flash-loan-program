use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use crate::instructions::helpers::{SignerAccount, ProgramAccount};
use crate::state::Config;
use core::mem::size_of;

/// #UpdateConfig
/// 
/// Update the Amm Config Account
/// 
/// Accounts:
/// 
/// 1. authority:                 [signer]
/// 2. config:                      [mut]
/// 
pub struct UpdateConfigAccounts<'a> {
    pub authority: &'a AccountInfo,
    pub config: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for UpdateConfigAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [authority, config] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Accounts Checks
        SignerAccount::check(authority)?;
        ProgramAccount::check(config)?;

        
        // Return the accounts
        Ok(Self {
            authority,
            config,
        })
    }
}

pub struct UpdateConfigAuthorityInstructionData {
    pub authority: [u8; 32],
}

impl<'a> TryFrom<&'a [u8]> for UpdateConfigAuthorityInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<[u8; 32]>() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let authority = data.try_into().unwrap();

        Ok(Self { authority })
    }
}

pub struct UpdateConfigFeeInstructionData {
    pub fee: u16,
}

impl<'a> TryFrom<&'a [u8]> for UpdateConfigFeeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<u16>() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let fee = u16::from_le_bytes(data.try_into().unwrap());
        Ok(Self { fee })
    }
}

pub enum UpdateConfigInstructionData {
    Authority(UpdateConfigAuthorityInstructionData),
    Fee(UpdateConfigFeeInstructionData),
    None,
}

pub struct UpdateConfig<'a> {
    pub accounts: UpdateConfigAccounts<'a>,
    pub instruction_data: UpdateConfigInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for UpdateConfig<'a> {
    type Error = ProgramError;
    
    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = UpdateConfigAccounts::try_from(accounts)?;
        
        let instruction_data = match data.len() {
            0 => UpdateConfigInstructionData::None,
            len if len == size_of::<[u8; 32]>() => {
                UpdateConfigInstructionData::Authority(UpdateConfigAuthorityInstructionData::try_from(data)?)
            },
            len if len == size_of::<u16>() => {
                UpdateConfigInstructionData::Fee(UpdateConfigFeeInstructionData::try_from(data)?)
            },
            _ => return Err(ProgramError::InvalidInstructionData),
        };
        
        // Verify that the authority is the correct authority and it's not set to "immutable"
        let data = accounts.config.try_borrow_data()?;
        let config = Config::load(&data)?;
        
        // Fix the match statement for authority check
        if config.authority == pinocchio_system::ID {
            return Err(ProgramError::Custom(1)); // Use Custom error code instead of InvalidAuthority
        }
        
        if config.authority != *accounts.authority.key() {
            return Err(ProgramError::Custom(1)); // Use Custom error code instead of InvalidAuthority
        }

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> UpdateConfig<'a> {
    pub const UPDATE_AUTHORITY_DISCRIMINATOR: &'a u8 = &4;
    pub const UPDATE_FEE_DISCRIMINATOR: &'a u8 = &5;
    pub const UDPATE_LOCK_STATUS_DISCRIMINATOR: &'a u8 = &6;
    pub const REMOVE_AUTHORITY_DISCRIMINATOR: &'a u8 = &7;
    
    pub fn process_update_authority(&mut self) -> ProgramResult {
        let mut data = self.accounts.config.try_borrow_mut_data()?;
        let config = Config::load_mut(data.as_mut())?;
        
        // Extract authority from the enum variant
        if let UpdateConfigInstructionData::Authority(auth_data) = &self.instruction_data {
            config.set_authority(auth_data.authority);
            Ok(())
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub fn process_update_fee(&mut self) -> ProgramResult {
        let mut data = self.accounts.config.try_borrow_mut_data()?;
        let config = Config::load_mut(data.as_mut())?;
        
        // Extract fee from the enum variant
        if let UpdateConfigInstructionData::Fee(fee_data) = &self.instruction_data {
            config.set_fee(fee_data.fee);
            Ok(())
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub fn process_update_lock_status(&mut self) -> ProgramResult {
        let mut data = self.accounts.config.try_borrow_mut_data()?;
        let config = Config::load_mut(data.as_mut())?;
        
        config.set_locked();
        Ok(())
    }

    pub fn process_remove_authority(&mut self) -> ProgramResult {
        let mut data = self.accounts.config.try_borrow_mut_data()?;
        let config = Config::load_mut(data.as_mut())?;
        
        config.set_authority(pinocchio_system::ID);
        Ok(())
    }
}