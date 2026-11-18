use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program::ID as SYSTEM_PROGRAM_ID,
        InstructionData, ToAccountMetas,
    },
    anchor_spl::associated_token::ID as ASSOCIATED_TOKEN_PROGRAM_ID,
    litesvm_token::spl_token::ID as TOKEN_PROGRAM_ID,
    solana_signer::Signer,
};

use crate::Pool;

pub fn create_initialise_ix(pool: &Pool, fee: u16, protocol_fee: u16) -> Instruction {
    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Initialize {
            seed: pool.seed,
            fee,
            protocol_fee,
            authority: Some(pool.admin.pubkey()),
        }
        .data(),
        ammq32026::accounts::Initialize {
            initializer: pool.admin.pubkey(),
            mint_x: pool.mint_x,
            mint_y: pool.mint_y,
            mint_lp: pool.mint_lp,
            vault_x: pool.vault_x,
            vault_y: pool.vault_y,
            config: pool.config,
            treasury: pool.treasury,
            treasury_x: pool.treasury_x,
            treasury_y: pool.treasury_y,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}
