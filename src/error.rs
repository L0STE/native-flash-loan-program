//! Error types

use pinocchio::program_error::ProgramError;

/// Errors that may be returned by the Token program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowError {
    
}

impl From<EscrowError> for ProgramError {
    fn from(e: EscrowError) -> Self {
        ProgramError::Custom(6000 + e as u32)
    }
}