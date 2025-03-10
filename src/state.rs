use pinocchio::{program_error::ProgramError, pubkey::Pubkey};
use core::mem::size_of;

#[repr(C)]
pub struct Config {
    pub seed: u64,
    pub authority: Pubkey,
    pub mint_x: Pubkey,           // Token X Mint
    pub mint_y: Pubkey,           // Token Y Mint
    pub fee: u16,                 // Swap fee in basis points
    pub locked: bool,
    pub config_bump: u8,
    pub lp_bump: u8,
    pub auth_bump: u8,
}

impl Config {
    pub const LEN: usize = size_of::<u64>() 
    + size_of::<Pubkey>() 
    + size_of::<Pubkey>() 
    + size_of::<Pubkey>() 
    + size_of::<u16>()
    + size_of::<bool>()
    + size_of::<u8>()
    + size_of::<u8>()
    + size_of::<u8>();

    #[inline(always)]
    pub fn load_mut(bytes: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if bytes.len() != Config::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(unsafe { &mut *core::mem::transmute::<*mut u8, *mut Self>(bytes.as_mut_ptr()) })
    }

    #[inline(always)]
    pub fn load(bytes: &[u8]) -> Result<&Self, ProgramError> {
        if bytes.len() != Config::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(unsafe { &*core::mem::transmute::<*const u8, *const Self>(bytes.as_ptr()) })
    }

    #[inline(always)]
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
    }

    #[inline(always)]
    pub fn set_authority(&mut self, authority: Pubkey) {
        self.authority = authority;
    }

    #[inline(always)]
    pub fn set_mint_x(&mut self, mint_x: Pubkey) {
        self.mint_x = mint_x;
    }

    #[inline(always)]
    pub fn set_mint_y(&mut self, mint_y: Pubkey) {
        self.mint_y = mint_y;
    }

    #[inline(always)]
    pub fn set_fee(&mut self, fee: u16) {
        self.fee = fee;
    }

    #[inline(always)]
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    #[inline(always)]
    pub fn set_inner(&mut self, seed: u64, authority: Pubkey, mint_x: Pubkey, mint_y: Pubkey, fee: u16, locked: bool, config_bump: u8, lp_bump: u8, auth_bump: u8) {
        self.seed = seed;
        self.authority = authority;
        self.mint_x = mint_x;
        self.mint_y = mint_y;
        self.fee = fee;
        self.locked = locked;
        self.config_bump = config_bump;
        self.lp_bump = lp_bump;
        self.auth_bump = auth_bump;
    }

    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        self.locked
    }    
}