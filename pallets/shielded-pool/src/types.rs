use crate::*;
use sp_std::convert::AsRef;

/// NoteCommitment is a 32-byte array that represents the commitment to a note.
#[derive(
    Encode, 
    Decode, 
    Clone, 
    PartialEq, 
    Eq, 
    RuntimeDebug, 
    TypeInfo, 
    MaxEncodedLen, 
    DecodeWithMemTracking
)]
pub struct NoteCommitment([u8; 32]);

impl AsRef<[u8]> for NoteCommitment {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for NoteCommitment {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Nullifier is a 32-byte array that represents the nullifier of a note.
#[derive(
    Encode, 
    Decode, 
    Clone, 
    PartialEq, 
    Eq, 
    RuntimeDebug, 
    PartialOrd,
    Ord,
    TypeInfo, 
    MaxEncodedLen, 
    DecodeWithMemTracking
)]
pub struct Nullifier([u8; 32]);

impl AsRef<[u8]> for Nullifier {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Nullifier {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Type of zero-knowledge proof being verified
#[derive(Encode, Decode, Debug, Clone, Copy, PartialEq, Eq, TypeInfo, DecodeWithMemTracking)]
pub enum ProofType {
    /// Proof for a new note commitment
    Commitment,
    /// Proof for spending an existing note
    Spend,
}


/// ImageId is a 32-byte array that represents the image ID of a proof.
pub type ImageId = [u32; 8];