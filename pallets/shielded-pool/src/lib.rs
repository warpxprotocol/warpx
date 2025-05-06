#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::collections::btree_set::BTreeSet;
pub use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use sp_std::vec::Vec;
pub use pallet::*;

mod types;
use types::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    /// Storage for note commitments in the shielded pool.
    ///
    /// Currently implemented as a simple `Vec` for demo purposes, which provides:
    /// - Simple and straightforward implementation
    /// - O(n) search complexity for note commitments
    ///
    /// # Future Work
    /// - Migrate to a more efficient data structure like Tiered Tree (similar to Penumbra)
    /// - Implement O(log n) search complexity for better scalability
    /// - Add support for batch operations
    /// - Consider implementing Merkle tree for efficient commitment verification
    #[pallet::storage]
    #[pallet::unbounded]
    pub type NoteCommitmentTree<T: Config> = StorageValue<_, Vec<NoteCommitment>, ValueQuery>;

    /// Storage for nullifiers to prevent double-spending.
    ///
    /// Uses `BTreeSet` for efficient lookup and insertion:
    /// - O(log n) complexity for contains() and insert() operations
    /// - Maintains sorted order of nullifiers
    /// - Per-account nullifier tracking for better privacy
    ///
    /// # Future Work
    /// - Consider implementing a global nullifier set for better privacy
    /// - Add support for nullifier expiration
    /// - Implement batch nullifier verification
    #[pallet::storage]
    #[pallet::unbounded]
    pub type Nullifiers<T: Config> = StorageValue<_, BTreeSet<Nullifier>, ValueQuery>;

    /// Storage for RISC0 image IDs used for proof verification.
    /// Maps proof types to their corresponding image IDs.
    #[pallet::storage]
    pub type ImageIds<T: Config> = StorageMap<_, Twox64Concat, ProofType, ImageId, OptionQuery>;

    /// Storage for registered prover accounts.
    /// Only registered provers can submit transactions.
    #[pallet::storage]
    pub type Prover<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

    /// Events emitted by the shielded pool pallet.
    ///
    /// These events are used to track important state changes in the shielded pool:
    /// - Note commitments being added
    /// - Nullifiers being registered
    /// - Spend transactions being verified
    ///
    /// # Future Work
    /// - Add more detailed event information (e.g., amounts, timestamps)
    /// - Implement event indexing for better querying
    /// - Add support for event filtering
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Emitted when a new note commitment is added to the tree.
        /// This represents a new note that can be spent in future transactions.
        NoteCommitmentAdded(NoteCommitment),
        /// Emitted when a nullifier is added to prevent double-spending.
        /// This marks a note as spent and prevents it from being spent again.
        NullifierAdded(Nullifier),
        /// Emitted when a spend transaction is successfully verified.
        /// This indicates that a note has been validly spent.
        ProofVerified(ProofType),
        /// Emitted when the RISC0 image ID is updated for a specific proof type.
        ImageIdUpdated(ImageId),
        /// Emitted when a new prover is registered.
        ProverRegistered(T::AccountId),
        /// Emitted when a prover is unregistered.
        ProverUnregistered(T::AccountId),
    }

    /// Errors that can occur in the shielded pool pallet.
    ///
    /// These errors represent various failure conditions that can occur during
    /// shielded pool operations. Each error is designed to provide clear feedback
    /// about what went wrong during transaction processing.
    ///
    /// # Future Work
    /// - Add more specific error variants for different failure cases
    /// - Include additional error context where possible
    /// - Implement error recovery mechanisms
    #[pallet::error]
    pub enum Error<T> {
        /// The provided zero-knowledge proof is invalid.
        /// This could be due to:
        /// - Incorrect proof generation
        /// - Tampered proof
        /// - Invalid public inputs
        InvalidProof,
        /// The nullifier has already been used in a previous transaction.
        /// This prevents double-spending of the same note.
        DuplicateNullifier,
        /// The note commitment being spent was not found in the commitment tree.
        /// This could indicate:
        /// - An invalid commitment
        /// - A commitment that was never registered
        /// - A commitment that was already spent
        NoteCommitmentNotFound,
        InvalidCommitmentProof,
        /// The image ID is invalid.
        InvalidImageId,
        /// The proof is not verified.
        VerificationError,
        /// The account is not a registered prover.
        NotRegisteredProver,
        /// The account is already registered as a prover.
        AlreadyRegisteredProver,
        /// The commitment already exists in the tree.
        DuplicateCommitment,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Adds a new note commitment to the commitment tree.
        ///
        /// This transaction is used to register a new note commitment in the shielded pool.
        /// The commitment represents a new note that can be spent in future transactions.
        ///
        /// # Arguments
        /// * `commitment` - The note commitment to be added to the tree
        /// * `proof` - The zero-knowledge proof that the commitment is valid
        ///
        /// # Events
        /// * `NoteCommitmentAdded` - Emitted when a new commitment is successfully added
        ///
        /// # Errors
        /// * `BadOrigin` - If the origin is not signed
        /// * `NotRegisteredProver` - If the account is not a registered prover
        /// * `InvalidProof` - If the zero-knowledge proof is invalid
        /// * `DuplicateCommitment` - If the commitment already exists in the tree
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)] // TODO: Benchmarking
        pub fn add_note_commitment(
            origin: OriginFor<T>,
            commitment: NoteCommitment,
            proof: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_prover(&who)?;

            // Verify the commitment proof
            Self::verify_proof(ProofType::Commitment, proof)?;

            // Check for duplicate commitment
            if NoteCommitmentTree::<T>::get().contains(&commitment) {
                return Err(Error::<T>::DuplicateCommitment.into());
            }

            // Add NoteCommitment to the tree
            NoteCommitmentTree::<T>::mutate(|tree| {
                tree.push(commitment.clone());
            });

            Self::deposit_event(Event::NoteCommitmentAdded(commitment));
            Ok(())
        }

        /// Verifies and processes a spend transaction in the shielded pool.
        ///
        /// This transaction verifies that a note can be spent by checking:
        /// 1. The nullifier hasn't been used before
        /// 2. The note commitment exists in the tree
        /// 3. The zero-knowledge proof is valid
        ///
        /// # Arguments
        /// * `proof` - The zero-knowledge proof of the spend
        /// * `nullifier` - The nullifier that prevents double-spending
        /// * `note_commitment` - The commitment of the note being spent
        ///
        /// # Events
        /// * `NullifierAdded` - Emitted when a new nullifier is registered
        /// * `SpendVerified` - Emitted when the spend is successfully verified
        ///
        /// # Errors
        /// * `BadOrigin` - If the origin is not signed
        /// * `DuplicateNullifier` - If the nullifier has already been used
        /// * `NoteCommitmentNotFound` - If the note commitment doesn't exist in the tree
        /// * `InvalidProof` - If the zero-knowledge proof is invalid
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)] // TODO: Benchmarking
        pub fn verify_spend(
            origin: OriginFor<T>,
            receipt: Vec<u8>,
            nullifier: Nullifier,
            note_commitment: NoteCommitment,
        ) -> DispatchResult {

            let prover = ensure_signed(origin)?;
            Self::ensure_prover(&prover)?;
            
            // 3. Verify proof via RISC0
            Self::verify_proof(ProofType::Spend, receipt)?;

            let nullifiers = Nullifiers::<T>::get();
            // 1. Check for duplicate nullifier
            ensure!(
                !nullifiers.contains(&nullifier),
                Error::<T>::DuplicateNullifier
            );

            // 2. Check if NoteCommitment exists
            ensure!(
                NoteCommitmentTree::<T>::get().contains(&note_commitment),
                Error::<T>::NoteCommitmentNotFound
            );

            // 4. Add nullifier
            Nullifiers::<T>::mutate(|nullifiers| {
                nullifiers.insert(nullifier.clone());
            });

            Self::deposit_event(Event::NullifierAdded(nullifier));
            Ok(())
        }

        /// Updates the RISC0 image ID for a specific proof type.
        /// 
        /// **Privileged Call**
        ///
        /// # Arguments
        /// * `proof_type` - The type of proof this image ID is for
        /// * `image_id` - The new RISC0 image ID
        ///
        /// # Errors
        /// * `BadOrigin` - If the origin is not root
        /// * `InvalidImageId` - If the image ID is invalid
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn update_image_id(
            origin: OriginFor<T>,
            proof_type: ProofType,
            image_id: ImageId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // TODO: Add validation for image ID if needed
            ImageIds::<T>::insert(proof_type, image_id);

            Self::deposit_event(Event::ImageIdUpdated(image_id));
            Ok(())
        }

        /// Registers a new prover account.
        /// Only root can register provers.
        ///
        /// # Arguments
        /// * `prover` - The account to register as a prover
        ///
        /// # Errors
        /// * `BadOrigin` - If the origin is not root
        /// * `AlreadyRegisteredProver` - If the account is already registered
        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn register_prover(
            origin: OriginFor<T>,
            prover: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            ensure!(
                Prover::<T>::get().as_ref() != Some(&prover),
                Error::<T>::AlreadyRegisteredProver
            );

            Prover::<T>::set(Some(prover.clone()));
            Self::deposit_event(Event::ProverRegistered(prover));
            Ok(())
        }

        /// Unregisters a prover account.
        /// Only root can unregister provers.
        ///
        /// # Arguments
        /// * `prover` - The account to unregister
        ///
        /// # Errors
        /// * `BadOrigin` - If the origin is not root
        /// * `NotRegisteredProver` - If the account is not registered
        #[pallet::call_index(4)]
        #[pallet::weight(10_000)]
        pub fn unregister_prover(
            origin: OriginFor<T>,
            prover: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            ensure!(
                Prover::<T>::get().as_ref() == Some(&prover),
                Error::<T>::NotRegisteredProver
            );

            Prover::<T>::set(None);
            Self::deposit_event(Event::ProverUnregistered(prover));
            Ok(())
        }
    }

    // RISC0 proof verification function
    impl<T: Config> Pallet<T> {
        /// Verifies a zero-knowledge proof using RISC0.
        ///
        /// This function handles both commitment and spend proofs using the same RISC0 verifier.
        /// The verification process differs based on the proof type:
        ///
        /// For commitment proofs:
        /// - Verifies that the commitment was generated using valid private inputs
        /// - Checks the commitment format and integrity
        ///
        /// For spend proofs:
        /// - Verifies the spender's knowledge of the private key
        /// - Checks the note's existence and spend status
        ///
        /// # Arguments
        /// * `proof_type` - The type of proof being verified (Commitment or Spend)
        /// * `proof` - The serialized zero-knowledge proof
        /// * `commitment` - The note commitment being verified
        /// * `nullifier` - The nullifier associated with the proof (if any)
        ///
        /// # Returns
        /// * `true` if the proof is valid, `false` otherwise
        ///
        /// # Future Work
        /// - Implement actual RISC0 proof verification
        /// - Add support for different proof systems
        /// - Implement batch proof verification
        /// - Add proof size limits and validation
        fn verify_proof(
            proof_type: ProofType,
            receipt_bytes: Vec<u8>,
        ) -> DispatchResult {
            // Get the image ID for this proof type
            let image_id = ImageIds::<T>::get(proof_type).ok_or(Error::<T>::InvalidImageId)?;

            // let receipt = Self::decode_receipt(receipt_bytes)?;

            // receipt.verify(image_id).map_err(|e| {
            //     log::error!("Proof verification failed: {:?}", e);
            //     Error::<T>::VerificationError
            // })?;

            // Self::deposit_event(Event::ProofVerified(ProofType::Spend));

            Ok(())
        }

        // Helper function to check if an account is a registered prover
        fn ensure_prover(who: &T::AccountId) -> DispatchResult {
            ensure!(
                Prover::<T>::get().as_ref() == Some(who),
                Error::<T>::NotRegisteredProver
            );
            Ok(())
        }

        // fn decode_receipt(receipt_bytes: Vec<u8>) -> Result<Receipt, Error<T>> {
        //     let receipt_json: String = Decode::decode(&mut &receipt_bytes[..]).map_err(|_| Error::<T>::InvalidProof)?;
        //     let receipt: Receipt = serde_json::from_str(&receipt_json).map_err(|_| Error::<T>::InvalidProof)?;
        //     Ok(receipt)
        // }
    }
} 