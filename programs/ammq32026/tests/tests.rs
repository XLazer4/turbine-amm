use {
    anchor_lang::{error::ERROR_CODE_OFFSET, AccountDeserialize},
    anchor_spl::associated_token,
    litesvm::{types::TransactionResult, LiteSVM},
    litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo},
    solana_keypair::Keypair,
    solana_message::{Instruction, Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

use ammq32026::{error::AmmError, state::Config};

mod ix_handlers;
use ix_handlers::*;

const SEED: u64 = 123;
const FEE: u16 = 30; // 0.30% to LPs
const PROTOCOL_FEE: u16 = 5; // 0.05% to the treasury
const STARTING_BALANCE: u64 = 1_000_000_000_000; // 1M tokens at 6 decimals

pub struct Pool {
    pub svm: LiteSVM,
    pub admin: Keypair,
    pub user: Keypair,
    pub seed: u64,
    pub treasury: Pubkey,
    pub mint_x: Pubkey,
    pub mint_y: Pubkey,
    pub config: Pubkey,
    pub mint_lp: Pubkey,
    pub vault_x: Pubkey,
    pub vault_y: Pubkey,
    pub treasury_x: Pubkey,
    pub treasury_y: Pubkey,
}

impl Pool {
    /// Fresh svm with the program loaded, two mints, and an admin + user that
    /// both hold STARTING_BALANCE of X and Y. The pool itself is not created.
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let bytes = include_bytes!(concat!(
            env!("CARGO_TARGET_TMPDIR"),
            "/../deploy/ammq32026.so"
        ));
        svm.add_program(ammq32026::id(), bytes).unwrap();

        let admin = Keypair::new();
        let user = Keypair::new();
        let treasury = Pubkey::new_unique();
        svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
        svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        let mint_x = CreateMint::new(&mut svm, &admin)
            .decimals(6)
            .authority(&admin.pubkey())
            .send()
            .unwrap();
        let mint_y = CreateMint::new(&mut svm, &admin)
            .decimals(6)
            .authority(&admin.pubkey())
            .send()
            .unwrap();

        for owner in [admin.pubkey(), user.pubkey()] {
            for mint in [mint_x, mint_y] {
                let ata = CreateAssociatedTokenAccount::new(&mut svm, &admin, &mint)
                    .owner(&owner)
                    .send()
                    .unwrap();
                MintTo::new(&mut svm, &admin, &mint, &ata, STARTING_BALANCE)
                    .send()
                    .unwrap();
            }
        }

        let config =
            Pubkey::find_program_address(&[b"config", &SEED.to_le_bytes()], &ammq32026::id()).0;
        let mint_lp = Pubkey::find_program_address(&[b"lp", config.as_ref()], &ammq32026::id()).0;

        Self {
            svm,
            admin,
            user,
            seed: SEED,
            treasury,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x: associated_token::get_associated_token_address(&config, &mint_x),
            vault_y: associated_token::get_associated_token_address(&config, &mint_y),
            treasury_x: associated_token::get_associated_token_address(&treasury, &mint_x),
            treasury_y: associated_token::get_associated_token_address(&treasury, &mint_y),
        }
    }

    /// Pool created and seeded by the admin with 1000 X / 4000 Y for 2000 LP.
    fn seeded() -> Self {
        let mut pool = Self::new();
        let init = create_initialise_ix(&pool, FEE, PROTOCOL_FEE);
        let admin = pool.admin.insecure_clone();
        pool.send(&[init], &admin).unwrap();
        let deposit = create_deposit_ix(&pool, &admin, 2_000_000_000, 1_000_000_000, 4_000_000_000);
        pool.send(&[deposit], &admin).unwrap();
        pool
    }

    fn send(&mut self, ixs: &[Instruction], signer: &Keypair) -> TransactionResult {
        self.svm.expire_blockhash();
        let blockhash = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(ixs, Some(&signer.pubkey()), &blockhash);
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
        self.svm.send_transaction(tx)
    }

    fn user_atas(&self, owner: &Pubkey) -> (Pubkey, Pubkey, Pubkey) {
        (
            associated_token::get_associated_token_address(owner, &self.mint_x),
            associated_token::get_associated_token_address(owner, &self.mint_y),
            associated_token::get_associated_token_address(owner, &self.mint_lp),
        )
    }

    /// Token account amount, read straight out of the account data (bytes 64..72).
    fn balance(&self, ata: &Pubkey) -> u64 {
        match self.svm.get_account(ata) {
            Some(acc) => u64::from_le_bytes(acc.data[64..72].try_into().unwrap()),
            None => 0,
        }
    }

    /// Mint supply lives at bytes 36..44.
    fn lp_supply(&self) -> u64 {
        let acc = self.svm.get_account(&self.mint_lp).unwrap();
        u64::from_le_bytes(acc.data[36..44].try_into().unwrap())
    }

    fn config(&self) -> Config {
        let acc = self.svm.get_account(&self.config).unwrap();
        Config::try_deserialize(&mut acc.data.as_slice()).unwrap()
    }

    fn reserves(&self) -> (u64, u64) {
        (self.balance(&self.vault_x), self.balance(&self.vault_y))
    }
}

fn assert_amm_error(res: TransactionResult, expected: AmmError) {
    let err = res.expect_err("transaction should have failed");
    let code = ERROR_CODE_OFFSET + expected as u32;
    let wanted = format!("custom program error: {:#x}", code);
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains(&wanted),
        "expected {:?} ({}), got {:?}\n{}",
        expected,
        wanted,
        err.err,
        logs
    );
}

// ---------------------------------------------------------------- initialize

#[test]
fn test_initialize() {
    let mut pool = Pool::new();
    let ix = create_initialise_ix(&pool, FEE, PROTOCOL_FEE);
    let admin = pool.admin.insecure_clone();
    pool.send(&[ix], &admin).unwrap();

    let config = pool.config();
    assert_eq!(config.seed, SEED);
    assert_eq!(config.authority, Some(admin.pubkey()));
    assert_eq!(config.mint_x, pool.mint_x);
    assert_eq!(config.mint_y, pool.mint_y);
    assert_eq!(config.treasury, pool.treasury);
    assert_eq!(config.fee, FEE);
    assert_eq!(config.protocol_fee, PROTOCOL_FEE);
    assert!(!config.locked);

    // vaults and treasury token accounts exist and are empty
    for ata in [pool.vault_x, pool.vault_y, pool.treasury_x, pool.treasury_y] {
        assert!(pool.svm.get_account(&ata).is_some());
        assert_eq!(pool.balance(&ata), 0);
    }
    assert_eq!(pool.lp_supply(), 0);
}

#[test]
fn test_initialize_rejects_fee_over_100_percent() {
    let mut pool = Pool::new();
    let ix = create_initialise_ix(&pool, 9_999, 1);
    let admin = pool.admin.insecure_clone();
    assert_amm_error(pool.send(&[ix], &admin), AmmError::FeePercentErr);
}

#[test]
fn test_initialize_twice_fails() {
    let mut pool = Pool::seeded();
    let ix = create_initialise_ix(&pool, FEE, PROTOCOL_FEE);
    let admin = pool.admin.insecure_clone();
    assert!(pool.send(&[ix], &admin).is_err());
}

// ------------------------------------------------------------------- deposit

#[test]
fn test_first_deposit_sets_price() {
    let pool = Pool::seeded();
    assert_eq!(pool.reserves(), (1_000_000_000, 4_000_000_000));
    assert_eq!(pool.lp_supply(), 2_000_000_000);
    let (_, _, admin_lp) = pool.user_atas(&pool.admin.pubkey());
    assert_eq!(pool.balance(&admin_lp), 2_000_000_000);
}

#[test]
fn test_deposit_is_proportional() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let (user_x, user_y, user_lp) = pool.user_atas(&user.pubkey());

    // 10% of supply -> 10% of each reserve
    let ix = create_deposit_ix(&pool, &user, 200_000_000, 100_000_000, 400_000_000);
    pool.send(&[ix], &user).unwrap();

    assert_eq!(pool.balance(&user_lp), 200_000_000);
    assert_eq!(pool.balance(&user_x), STARTING_BALANCE - 100_000_000);
    assert_eq!(pool.balance(&user_y), STARTING_BALANCE - 400_000_000);
    assert_eq!(pool.reserves(), (1_100_000_000, 4_400_000_000));
    assert_eq!(pool.lp_supply(), 2_200_000_000);
}

#[test]
fn test_deposit_slippage() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    // needs 100 X, user only allows 99.99
    let ix = create_deposit_ix(&pool, &user, 200_000_000, 99_990_000, 400_000_000);
    assert_amm_error(pool.send(&[ix], &user), AmmError::SlippageExceeded);
}

#[test]
fn test_deposit_zero_fails() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let ix = create_deposit_ix(&pool, &user, 0, 1, 1);
    assert_amm_error(pool.send(&[ix], &user), AmmError::InvalidAmount);
}

#[test]
fn test_tiny_deposit_still_pays() {
    // 1 LP unit of a 2000 LP supply should never be free
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let (user_x, user_y, _) = pool.user_atas(&user.pubkey());
    let ix = create_deposit_ix(&pool, &user, 1, 10, 10);
    pool.send(&[ix], &user).unwrap();
    assert!(pool.balance(&user_x) < STARTING_BALANCE);
    assert!(pool.balance(&user_y) < STARTING_BALANCE);
}

// ------------------------------------------------------------------ withdraw

#[test]
fn test_withdraw_partial() {
    let mut pool = Pool::seeded();
    let admin = pool.admin.insecure_clone();
    let (admin_x, admin_y, admin_lp) = pool.user_atas(&admin.pubkey());
    let x_before = pool.balance(&admin_x);
    let y_before = pool.balance(&admin_y);

    // burn a quarter of the supply
    let ix = create_withdraw_ix(&pool, &admin, 500_000_000, 250_000_000, 1_000_000_000);
    pool.send(&[ix], &admin).unwrap();

    assert_eq!(pool.balance(&admin_lp), 1_500_000_000);
    assert_eq!(pool.balance(&admin_x), x_before + 250_000_000);
    assert_eq!(pool.balance(&admin_y), y_before + 1_000_000_000);
    assert_eq!(pool.reserves(), (750_000_000, 3_000_000_000));
    assert_eq!(pool.lp_supply(), 1_500_000_000);
}

#[test]
fn test_withdraw_everything() {
    let mut pool = Pool::seeded();
    let admin = pool.admin.insecure_clone();
    let ix = create_withdraw_ix(&pool, &admin, 2_000_000_000, 0, 0);
    pool.send(&[ix], &admin).unwrap();
    assert_eq!(pool.reserves(), (0, 0));
    assert_eq!(pool.lp_supply(), 0);
}

#[test]
fn test_withdraw_slippage() {
    let mut pool = Pool::seeded();
    let admin = pool.admin.insecure_clone();
    let ix = create_withdraw_ix(&pool, &admin, 500_000_000, 250_000_001, 0);
    assert_amm_error(pool.send(&[ix], &admin), AmmError::SlippageExceeded);
}

#[test]
fn test_withdraw_zero_fails() {
    let mut pool = Pool::seeded();
    let admin = pool.admin.insecure_clone();
    let ix = create_withdraw_ix(&pool, &admin, 0, 0, 0);
    assert_amm_error(pool.send(&[ix], &admin), AmmError::InvalidAmount);
}

#[test]
fn test_withdraw_more_than_owned_fails() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    // user has no LP yet, deposit a little then try to burn more
    let ix = create_deposit_ix(&pool, &user, 100, 1_000, 1_000);
    pool.send(&[ix], &user).unwrap();
    let ix = create_withdraw_ix(&pool, &user, 101, 0, 0);
    assert!(pool.send(&[ix], &user).is_err());
}

// ---------------------------------------------------------------------- swap

#[test]
fn test_swap_x_for_y() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let (user_x, user_y, _) = pool.user_atas(&user.pubkey());
    let (rx, ry) = pool.reserves();

    let amount_in = 10_000_000; // 10 X
    let protocol_cut = amount_in * PROTOCOL_FEE as u64 / 10_000;
    let to_pool = amount_in - protocol_cut;
    let after_fee = to_pool as u128 * (10_000 - FEE) as u128 / 10_000;
    let expected_out = (ry as u128 * after_fee / (rx as u128 + after_fee)) as u64;

    let ix = create_swap_ix(&pool, &user, true, amount_in, expected_out);
    pool.send(&[ix], &user).unwrap();

    assert_eq!(pool.balance(&user_x), STARTING_BALANCE - amount_in);
    assert_eq!(pool.balance(&user_y), STARTING_BALANCE + expected_out);
    assert_eq!(pool.balance(&pool.treasury_x), protocol_cut);
    assert_eq!(pool.balance(&pool.treasury_y), 0);
    assert_eq!(pool.reserves(), (rx + to_pool, ry - expected_out));

    // LP fee stays in the pool, so k goes up
    let (nx, ny) = pool.reserves();
    assert!(nx as u128 * ny as u128 > rx as u128 * ry as u128);
}

#[test]
fn test_swap_y_for_x() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let (user_x, user_y, _) = pool.user_atas(&user.pubkey());
    let (rx, ry) = pool.reserves();

    let amount_in = 40_000_000; // 40 Y
    let protocol_cut = amount_in * PROTOCOL_FEE as u64 / 10_000;
    let to_pool = amount_in - protocol_cut;
    let after_fee = to_pool as u128 * (10_000 - FEE) as u128 / 10_000;
    let expected_out = (rx as u128 * after_fee / (ry as u128 + after_fee)) as u64;

    let ix = create_swap_ix(&pool, &user, false, amount_in, expected_out);
    pool.send(&[ix], &user).unwrap();

    assert_eq!(pool.balance(&user_y), STARTING_BALANCE - amount_in);
    assert_eq!(pool.balance(&user_x), STARTING_BALANCE + expected_out);
    assert_eq!(pool.balance(&pool.treasury_y), protocol_cut);
    assert_eq!(pool.balance(&pool.treasury_x), 0);
    assert_eq!(pool.reserves(), (rx - expected_out, ry + to_pool));
}

#[test]
fn test_swap_slippage() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    // 10 X at a 1:4 price can never get 40 Y back once fees and price impact kick in
    let ix = create_swap_ix(&pool, &user, true, 10_000_000, 40_000_000);
    assert_amm_error(pool.send(&[ix], &user), AmmError::SlippageExceeded);
}

#[test]
fn test_swap_zero_fails() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let ix = create_swap_ix(&pool, &user, true, 0, 0);
    assert_amm_error(pool.send(&[ix], &user), AmmError::InvalidAmount);
}

#[test]
fn test_swap_on_empty_pool_fails() {
    let mut pool = Pool::new();
    let admin = pool.admin.insecure_clone();
    let ix = create_initialise_ix(&pool, FEE, PROTOCOL_FEE);
    pool.send(&[ix], &admin).unwrap();
    let ix = create_swap_ix(&pool, &admin, true, 1_000_000, 0);
    assert_amm_error(pool.send(&[ix], &admin), AmmError::NoLiquidityInPool);
}

#[test]
fn test_swap_wrong_treasury_fails() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    // point the treasury accounts at the user's own token accounts
    let (user_x, user_y, _) = pool.user_atas(&user.pubkey());
    let real = (pool.treasury_x, pool.treasury_y);
    pool.treasury_x = user_x;
    pool.treasury_y = user_y;
    let ix = create_swap_ix(&pool, &user, true, 1_000_000, 0);
    (pool.treasury_x, pool.treasury_y) = real;
    assert!(pool.send(&[ix], &user).is_err());
}

#[test]
fn test_lps_earn_fees() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let admin = pool.admin.insecure_clone();

    // round trip a bunch of volume through the pool
    for _ in 0..5 {
        let ix = create_swap_ix(&pool, &user, true, 50_000_000, 0);
        pool.send(&[ix], &user).unwrap();
        let ix = create_swap_ix(&pool, &user, false, 200_000_000, 0);
        pool.send(&[ix], &user).unwrap();
    }

    let (admin_x, admin_y, _) = pool.user_atas(&admin.pubkey());
    let x_before = pool.balance(&admin_x);
    let y_before = pool.balance(&admin_y);
    let ix = create_withdraw_ix(&pool, &admin, 2_000_000_000, 0, 0);
    pool.send(&[ix], &admin).unwrap();
    let got_x = pool.balance(&admin_x) - x_before;
    let got_y = pool.balance(&admin_y) - y_before;

    // value everything in Y at the starting 1:4 price, LP should be up
    let deposited = 1_000_000_000u128 * 4 + 4_000_000_000;
    let withdrawn = got_x as u128 * 4 + got_y as u128;
    assert!(withdrawn > deposited, "{} <= {}", withdrawn, deposited);

    // and the treasury picked up its cut on both sides
    assert!(pool.balance(&pool.treasury_x) > 0);
    assert!(pool.balance(&pool.treasury_y) > 0);
}

// ------------------------------------------------------------- lock / unlock

#[test]
fn test_lock_blocks_pool() {
    let mut pool = Pool::seeded();
    let admin = pool.admin.insecure_clone();
    let user = pool.user.insecure_clone();

    let ix = create_lock_ix(&pool, &admin);
    pool.send(&[ix], &admin).unwrap();
    assert!(pool.config().locked);

    let ix = create_deposit_ix(&pool, &user, 200_000_000, u64::MAX, u64::MAX);
    assert_amm_error(pool.send(&[ix], &user), AmmError::PoolLocked);
    let ix = create_swap_ix(&pool, &user, true, 1_000_000, 0);
    assert_amm_error(pool.send(&[ix], &user), AmmError::PoolLocked);
    let ix = create_withdraw_ix(&pool, &admin, 1_000, 0, 0);
    assert_amm_error(pool.send(&[ix], &admin), AmmError::PoolLocked);
}

#[test]
fn test_unlock_restores_pool() {
    let mut pool = Pool::seeded();
    let admin = pool.admin.insecure_clone();
    let user = pool.user.insecure_clone();

    let ix = create_lock_ix(&pool, &admin);
    pool.send(&[ix], &admin).unwrap();
    let ix = create_unlock_ix(&pool, &admin);
    pool.send(&[ix], &admin).unwrap();
    assert!(!pool.config().locked);

    let ix = create_swap_ix(&pool, &user, true, 1_000_000, 0);
    pool.send(&[ix], &user).unwrap();
}

#[test]
fn test_lock_requires_authority() {
    let mut pool = Pool::seeded();
    let user = pool.user.insecure_clone();
    let ix = create_lock_ix(&pool, &user);
    assert_amm_error(pool.send(&[ix], &user), AmmError::InvalidAuthority);
    let ix = create_unlock_ix(&pool, &user);
    assert_amm_error(pool.send(&[ix], &user), AmmError::InvalidAuthority);
}
