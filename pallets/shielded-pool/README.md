# Verifier Pallet

This pallet implements a verifier for private transactions using RISC0 zero-knowledge proofs.

## Features

- NoteCommitment management
- Nullifier tracking
- RISC0 proof verification
- Private transaction verification

## Usage

### Adding a NoteCommitment

```rust
let commitment = NoteCommitment([0u8; 32]);
Verifier::add_note_commitment(origin, commitment);
```

### Verifying a Spend Transaction

```rust
let proof = vec![0u8; 32];
let nullifier = Nullifier([0u8; 32]);
let note_commitment = NoteCommitment([0u8; 32]);
Verifier::verify_spend(origin, proof, nullifier, note_commitment);
```

## Events

- `NoteCommitmentAdded`: Emitted when a new NoteCommitment is added
- `NullifierAdded`: Emitted when a new nullifier is added
- `SpendVerified`: Emitted when a spend transaction is verified

## Errors

- `InvalidProof`: The provided proof is invalid
- `DuplicateNullifier`: The nullifier has already been used
- `NoteCommitmentNotFound`: The NoteCommitment is not found in the tree
