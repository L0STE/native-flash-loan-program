use pinocchio::{account_info::AccountInfo, instruction::{Seed, Signer}, program_error::ProgramError, pubkey::find_program_address, sysvars::{rent::Rent, Sysvar}, ProgramResult};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::{InitializeAccount3, InitializeMint2};
use pinocchio_associated_token_account::instructions::Create;

extern "C" {
    fn sol_log_(
        input: *const u8, 
        len: u64
    ) -> u64;
}

pub enum QuasarAccountError {
    AccountNotSigner,
    AccountNotOwnedByProgram,
    AccountNotOwnedBySystemProgram,
    AccountNotOwnedByTokenProgram,
    InvalidTokenAccountData,
    InvalidAssociatedTokenAddress,
}

impl From<QuasarAccountError> for ProgramError {
    fn from(e: QuasarAccountError) -> Self {
        ProgramError::Custom(3000 + e as u32)
    }
}

pub struct SignerAccount;

impl SignerAccount {
    pub fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if !account.is_signer() {
            unsafe {
                sol_log_(format!("Account {:?} is not a signer", account.key()).as_ptr(), 56);
            }
            return Err(QuasarAccountError::AccountNotSigner.into());
        }
        Ok(())
    }
}

pub struct SystemAccount;

impl SystemAccount {
    pub fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&pinocchio_system::ID) {
            unsafe {
                sol_log_(format!("Account {:?} is not owned by the System Program", account.key()).as_ptr(), 75);
            }
            return Err(QuasarAccountError::AccountNotOwnedBySystemProgram.into());
        }
        Ok(())
    }
}

pub struct MintAccount;

// Do we need a discriminator check here? Does Mint have a discriminator?
impl MintAccount {
    pub fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&pinocchio_token::ID) {
            unsafe {
                sol_log_(format!("Account {:?} is not owned by the Token Program", account.key()).as_ptr(), 74);
            }
            return Err(QuasarAccountError::AccountNotOwnedByTokenProgram.into());
        }

        if account.data_len() != pinocchio_token::state::Mint::LEN {
            unsafe {
                sol_log_(format!("Account {:?} is not a Mint Account", account.key()).as_ptr(), 62);
            }
            return Err(QuasarAccountError::InvalidTokenAccountData.into());
        }

        Ok(())
    }

    pub fn init(account: &AccountInfo, payer: &AccountInfo, decimals: u8, mint_authority: &[u8; 32], freeze_authority: Option<&[u8; 32]>) -> ProgramResult {
        // Get required lamports for rent
        let lamports = Rent::get()?.minimum_balance(pinocchio_token::state::Mint::LEN);

        // Fund the account with the required lamports
        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: pinocchio_token::state::Mint::LEN as u64,
            owner: &crate::ID,
        }.invoke()?;
        
        InitializeMint2 {
            mint: account,
            decimals,
            mint_authority,
            freeze_authority,
        }.invoke()
    }

    pub fn init_with_seeds(account: &AccountInfo, payer: &AccountInfo, decimals: u8, mint_authority: &[u8; 32], freeze_authority: Option<&[u8; 32]>, seeds: &[Seed]) -> ProgramResult {
        // Get required lamports for rent
        let lamports = Rent::get()?.minimum_balance(pinocchio_token::state::Mint::LEN);

        // Fund the account with the required lamports
        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: pinocchio_token::state::Mint::LEN as u64,
            owner: &crate::ID,
        }
        .invoke()?;

        let signer_seeds = [Signer::from(seeds)];
        InitializeMint2 {
            mint: account,
            decimals,
            mint_authority,
            freeze_authority,
        }.invoke_signed(&signer_seeds)
    }

    pub fn supply(account: &AccountInfo) -> u64 {
        let data = account.try_borrow_data().unwrap();
        u64::from_le_bytes(data[36..44].try_into().unwrap())
    }
}

pub struct TokenAccount;

impl TokenAccount {
    pub fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&pinocchio_token::ID) {
            unsafe {
                sol_log_(format!("Account {:?} is not owned by the Token Program", account.key()).as_ptr(), 74);
            }
            return Err(QuasarAccountError::AccountNotOwnedByTokenProgram.into());
        }

        if account.data_len() != pinocchio_token::state::TokenAccount::LEN {
            unsafe {
                sol_log_(format!("Account {:?} is not a Token Account", account.key()).as_ptr(), 63);
            }
            return Err(QuasarAccountError::InvalidTokenAccountData.into());
        }

        Ok(())
    }

    pub fn init(account: &AccountInfo, mint: &AccountInfo, payer: &AccountInfo, owner: &[u8; 32]) -> ProgramResult {
        // Get required lamports for rent
        let lamports = Rent::get()?.minimum_balance(pinocchio_token::state::TokenAccount::LEN);

        // Fund the account with the required lamports
        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: pinocchio_token::state::TokenAccount::LEN as u64,
            owner: &pinocchio_token::ID,
        }.invoke()?;

        // Initialize the Token Account
        InitializeAccount3 {
            account,
            mint,
            owner,
        }.invoke()
    }

    pub fn init_if_needed(account: &AccountInfo, mint: &AccountInfo, payer: &AccountInfo, owner: &[u8; 32]) -> ProgramResult {
        match Self::check(account) {
            Ok(_) => Ok(()),
            Err(_) => Self::init(account, mint, payer, owner),
        }
    }

    pub fn get_amount(account: &AccountInfo) -> u64 {
        let data = account.try_borrow_data().unwrap();
        u64::from_le_bytes(data[64..72].try_into().unwrap())
    }
}
pub struct AssociatedTokenAccount;

impl AssociatedTokenAccount {
    /// Check if an account is an associated token account
    pub fn check(account: &AccountInfo, authority: &AccountInfo, mint: &AccountInfo) -> Result<(), ProgramError> {
        TokenAccount::check(account)?;

        if find_program_address(&[authority.key(), &pinocchio_token::ID, mint.key()], &pinocchio_associated_token_account::ID).0.ne(account.key()) {
            unsafe {
                sol_log_(format!("Account {:?} is not an associated token account", account.key()).as_ptr(), 75);
            }
            return Err(QuasarAccountError::InvalidAssociatedTokenAddress.into());
        }

        Ok(())
    }

    /// Initialize a token account with a mint and owner
    pub fn init(account: &AccountInfo, mint: &AccountInfo, payer: &AccountInfo, owner: &AccountInfo, system_program: &AccountInfo, token_program: &AccountInfo) -> ProgramResult {
        Create {
            funding_account: payer,
            account,
            wallet: owner,
            mint,
            system_program,
            token_program,
        }.invoke()
    }

    pub fn init_if_needed(account: &AccountInfo, mint: &AccountInfo, payer: &AccountInfo, owner: &AccountInfo, system_program: &AccountInfo, token_program: &AccountInfo) -> ProgramResult {
        match Self::check(account, payer, mint) {
            Ok(_) => Ok(()),
            Err(_) => Self::init(account, mint, payer, owner, system_program, token_program),
        }
    }

}

pub struct ProgramAccount;

impl ProgramAccount {
    /// Check if an account is a program account
    pub fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        
        if account.owner().ne(&crate::ID) {
            unsafe {
                sol_log_(format!("Account {:?} is not owned by the Program", account.key()).as_ptr(), 75);
            }
            return Err(QuasarAccountError::AccountNotOwnedByProgram.into());
        }

        Ok(())
    }

    /// Create a new program account
    pub fn init<'a, T: Sized>(
        payer: &AccountInfo,
        account: &AccountInfo,
        seeds: &[Seed<'a>],
        space: usize,
    ) -> ProgramResult {
        // Get required lamports for rent
        let lamports = Rent::get()?.minimum_balance(space);

        // Create signer with seeds slice
        let signer = [Signer::from(seeds)];

        // Create the account
        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: space as u64,
            owner: &crate::ID,
        }
        .invoke_signed(&signer)?;

        Ok(())
    }

    pub fn close(account: &AccountInfo, destination: &AccountInfo) -> ProgramResult {
        *destination.try_borrow_mut_lamports()? += *account.try_borrow_lamports()?;
        account.realloc(0, true)?;
        account.close()
    }

    // Need to add the has_one functions
}