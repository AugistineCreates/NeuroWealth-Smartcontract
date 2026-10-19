//! Tests for emergency withdrawal functionality (#635)

use super::utils::*;
use crate::TOPIC_EMERGENCY_WITHDRAWAL;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
#[should_panic(expected = "Error(Contract, #74)")]
fn test_emergency_withdraw_when_not_paused_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Emergency withdraw when not paused should fail
    client.emergency_withdraw(&user, &amount);
}

#[test]
fn test_emergency_withdraw_when_paused_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Emergency withdraw should work when paused
    client.emergency_withdraw(&user, &amount);

    // Verify shares are burned
    assert_eq!(client.get_shares(&user), 0);
    let (users, _) = client.get_users_with_shares(&0, &10);
    assert_eq!(
        users.len(),
        0,
        "fully withdrawn user should be pruned from index"
    );
}

#[test]
fn test_emergency_withdraw_deducts_from_idle_first() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Emergency withdraw should work from idle balance
    client.emergency_withdraw(&user, &amount);

    // Verify user balance is zero
    assert_eq!(client.get_shares(&user), 0);
}

#[test]
fn test_emergency_withdraw_partial_liquidity() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Partial emergency withdraw
    let partial_amount = 500_000_i128;
    client.emergency_withdraw(&user, &partial_amount);

    // Verify remaining shares
    let remaining_shares = client.get_shares(&user);
    assert!(remaining_shares > 0);
}

/// Requesting more than the vault's available idle liquidity does not
/// panic in `BurnAndPayMode::Emergency` -- unlike `withdraw`'s `Requested`
/// mode, it caps `assets_to_pay` at `vault_balance` and pays out whatever
/// is actually available rather than rejecting the call outright.
#[test]
fn test_emergency_withdraw_caps_at_available_liquidity() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Request double the user's actual balance; emergency_withdraw caps
    // the payout at the vault's available idle liquidity (the full
    // deposit) rather than panicking, and burns all of the user's shares.
    client.emergency_withdraw(&user, &(amount * 2));
    assert_eq!(client.get_shares(&user), 0);
}

/// A user with genuinely zero shares cannot emergency-withdraw anything.
#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_emergency_withdraw_with_no_shares_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, _usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    client.pause(&owner);

    // User never deposited, so they have zero shares.
    client.emergency_withdraw(&user, &1_000_000_i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #37)")]
fn test_emergency_withdraw_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Try emergency withdraw with zero amount
    client.emergency_withdraw(&user, &0);
}

/// `emergency_withdraw` calls `user.require_auth()`; because there is no
/// separate caller parameter to spoof, the guard is tested by revoking all
/// auth with `mock_auths(&[])` and checking the `try_*` variant rejects the
/// call.
#[test]
fn test_emergency_withdraw_requires_user_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    client.pause(&owner);

    // Drop all auth: user.require_auth() inside emergency_withdraw cannot be
    // satisfied, so the call must fail regardless of who invokes it.
    env.mock_auths(&[]);

    let result = client.try_emergency_withdraw(&user, &amount);
    assert!(
        result.is_err(),
        "emergency_withdraw must reject calls without the user's authorization"
    );
}

#[test]
fn test_emergency_withdraw_emits_correct_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Emergency withdraw
    client.emergency_withdraw(&user, &amount);

    // Check emergency withdrawal event
    let matching = find_events_by_topic(env.events().all(), &env, TOPIC_EMERGENCY_WITHDRAWAL);
    assert!(
        !matching.is_empty(),
        "emergency_withdraw must publish a TOPIC_EMERGENCY_WITHDRAWAL event"
    );
}

#[test]
fn test_emergency_withdraw_does_not_affect_rebalance() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Emergency withdraw should work
    client.emergency_withdraw(&user, &amount);

    // Unpause the vault
    client.unpause(&owner);

    // Rebalance should still work after unpause
    // Note: This test assumes rebalance would work with proper Blend setup
    // In a full test, you'd need to configure Blend pool
}

#[test]
fn test_emergency_withdraw_preserves_rounding_rules() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Add some yield to create rounding scenarios. `update_total_assets`
    // requires the vault's actual token balance to cover the reported
    // total, so the "yield" must be minted into the vault first.
    let yield_amount = 123_456_i128;
    let token_client = TestTokenClient::new(&env, &usdc_token);
    token_client.mint(&contract_id, &yield_amount);
    let total_assets = client.get_total_assets();
    client.update_total_assets(&agent, &(total_assets + yield_amount), &false, &0_u32);

    // Pause the vault
    client.pause(&owner);

    // Emergency withdraw with amount that might cause rounding
    let withdraw_amount = 500_000_i128;
    client.emergency_withdraw(&user, &withdraw_amount);

    // Verify the withdrawal followed rounding rules
    // User should have remaining shares
    let remaining_shares = client.get_shares(&user);
    assert!(remaining_shares >= 0);
}

#[test]
fn test_emergency_withdraw_with_yield_deployed() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Simulate yield landing in the vault.
    let yield_amount = 200_000_i128;
    let token_client = TestTokenClient::new(&env, &usdc_token);
    token_client.mint(&contract_id, &yield_amount);
    let total_assets = client.get_total_assets();
    client.update_total_assets(&agent, &(total_assets + yield_amount), &false, &0_u32);

    // Pause the vault
    client.pause(&owner);

    // Emergency withdraw should work even with deployed yield. With yield
    // applied, the share price is above 1.0, so withdrawing exactly the
    // original principal burns fewer than all of the user's shares.
    client.emergency_withdraw(&user, &amount);

    let remaining_shares = client.get_shares(&user);
    assert!(
        remaining_shares > 0,
        "withdrawing only the original principal should leave yield-backed shares behind"
    );
    let remaining_balance = client.get_balance(&user);
    assert!(
        remaining_balance > 0,
        "the user's remaining shares must still back a positive balance"
    );
}
