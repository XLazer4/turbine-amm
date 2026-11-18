use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program::ID as SYSTEM_PROGRAM_ID,
        InstructionData, ToAccountMetas,
    },
    anchor_spl::associated_token::ID as ASSOCIATED_TOKEN_PROGRAM_ID,
    litesvm_token::spl_token::ID as TOKEN_PROGRAM_ID,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

use crate::Pool;

pub fn create_withdraw_ix(
    pool: &Pool,
    user: &Keypair,
    amount: u64,
    min_x: u64,
    min_y: u64,
) -> Instruction {
    let (user_x, user_y, user_lp) = pool.user_atas(&user.pubkey());

    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Withdraw {
            amount,
            min_x,
            min_y,
        }
        .data(),
        ammq32026::accounts::Withdraw {
            user: user.pubkey(),
            mint_x: pool.mint_x,
            mint_y: pool.mint_y,
            config: pool.config,
            mint_lp: pool.mint_lp,
            vault_x: pool.vault_x,
            vault_y: pool.vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}
