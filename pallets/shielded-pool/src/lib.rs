#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_std::vec::Vec;
    use sp_std::collections::btree_set::BTreeSet;

    // NoteCommitment와 Nullifier 타입 정의
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct NoteCommitment([u8; 32]);

    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct Nullifier([u8; 32]);

    // 팔렛 설정
    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    // 팔렛 상태 정의
    #[pallet::storage]
    pub type NoteCommitmentTree<T> = StorageValue<_, Vec<NoteCommitment>, ValueQuery>;

    #[pallet::storage]
    pub type Nullifiers<T> = StorageValue<_, BTreeSet<Nullifier>, ValueQuery>;

    // 이벤트 정의
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        NoteCommitmentAdded(NoteCommitment),
        NullifierAdded(Nullifier),
        SpendVerified(T::AccountId),
    }

    // 에러 정의
    #[pallet::error]
    pub enum Error<T> {
        InvalidProof,
        DuplicateNullifier,
        NoteCommitmentNotFound,
    }

    // 팔렛 구현
    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // 팔렛 함수 구현
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // NoteCommitment 추가
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn add_note_commitment(
            origin: OriginFor<T>,
            commitment: NoteCommitment,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;

            // NoteCommitment 트리에 추가
            NoteCommitmentTree::<T>::mutate(|tree| {
                tree.push(commitment.clone());
            });

            Self::deposit_event(Event::NoteCommitmentAdded(commitment));
            Ok(())
        }

        // Spend 트랜잭션 검증
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn verify_spend(
            origin: OriginFor<T>,
            proof: Vec<u8>,
            nullifier: Nullifier,
            note_commitment: NoteCommitment,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 1. nullifier 중복 체크
            ensure!(
                !Nullifiers::<T>::get().contains(&nullifier),
                Error::<T>::DuplicateNullifier
            );

            // 2. NoteCommitment 존재 체크
            ensure!(
                NoteCommitmentTree::<T>::get().contains(&note_commitment),
                Error::<T>::NoteCommitmentNotFound
            );

            // 3. RISC0 증명 검증 (실제 구현 필요)
            if !Self::verify_proof(&proof, &nullifier, &note_commitment) {
                return Err(Error::<T>::InvalidProof.into());
            }

            // 4. nullifier 추가
            Nullifiers::<T>::mutate(|nullifiers| {
                nullifiers.insert(nullifier.clone());
            });

            Self::deposit_event(Event::NullifierAdded(nullifier));
            Self::deposit_event(Event::SpendVerified(who));
            Ok(())
        }
    }

    // RISC0 증명 검증 함수 (실제 구현 필요)
    impl<T: Config> Pallet<T> {
        fn verify_proof(
            proof: &[u8],
            nullifier: &Nullifier,
            note_commitment: &NoteCommitment,
        ) -> bool {
            // RISC0 증명 검증 로직 구현
            // 예: receipt.verify() 등
            true // 임시 구현
        }
    }
} 