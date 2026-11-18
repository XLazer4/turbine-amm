use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    litesvm_token::spl_token::ID as TOKEN_PROGRAM_ID,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

use crate::Pool;

pub fn create_swap_ix(
    pool: &Pool,
    user: &Keypair,
    is_x: bool,
    amount: u64,
    min: u64,
) -> Instruction {
    let (user_x, user_y, _) = pool.user_atas(&user.pubkey());

    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Swap { is_x, amount, min }.data(),
        ammq32026::accounts::Swap {
            user: user.pubkey(),
            mint_x: pool.mint_x,
            mint_y: pool.mint_y,
            config: pool.config,
            vault_x: pool.vault_x,
            vault_y: pool.vault_y,
            user_x,
            user_y,
            treasury_x: pool.treasury_x,
            treasury_y: pool.treasury_y,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}
