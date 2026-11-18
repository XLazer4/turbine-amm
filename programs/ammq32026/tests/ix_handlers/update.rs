use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

use crate::Pool;

fn update_accounts(
    pool: &Pool,
    authority: &Keypair,
) -> Vec<anchor_lang::solana_program::instruction::AccountMeta> {
    ammq32026::accounts::Update {
        authority: authority.pubkey(),
        config: pool.config,
    }
    .to_account_metas(None)
}

pub fn create_lock_ix(pool: &Pool, authority: &Keypair) -> Instruction {
    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Lock {}.data(),
        update_accounts(pool, authority),
    )
}

pub fn create_unlock_ix(pool: &Pool, authority: &Keypair) -> Instruction {
    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Unlock {}.data(),
        update_accounts(pool, authority),
    )
}
