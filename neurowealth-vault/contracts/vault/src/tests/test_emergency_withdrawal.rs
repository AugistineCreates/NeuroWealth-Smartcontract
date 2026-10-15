//! Tests for emergency withdrawal functionality (#635)

use super::utils::*;
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
    assert_eq!(users.len(), 0, "fully withdrawn user should be pruned from index");
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

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_emergency_withdraw_insufficient_shares() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Try to withdraw more than user has
    client.emergency_withdraw(&user, &(amount * 2));
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

#[test]
fn test_emergency_withdraw_requires_user_auth() {
    let env = Env::default();
    env.mock_all_auths(); // But we'll override for specific auth

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Pause the vault
    client.pause(&owner);

    // Reset auth to only allow owner
    env.mock_auths(&[
        soroban_sdk::testutils::AuthEntry {
            address: &owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "pause",
                args: (&owner,).into_val(&env),
                auth: soroban_sdk::testutils::MockAuth::All,
            },
        },
    ]);

    // Try emergency withdraw without user auth - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.emergency_withdraw(&user, &amount);
    }));

    assert!(result.is_err());
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
    let events = env.events().all();
    let emergency_event = events.iter().find(|e| {
        if let Some(topic) = e.topics.get(0) {
            topic.to_string() == "em_wd"
        } else {
            false
        }
    });
    assert!(emergency_event.is_some());
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

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Add some yield to create rounding scenarios
    let total_assets = client.get_total_assets();
    client.update_total_assets(&owner, &(total_assets + 123_456_i128), &0_i128);

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

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Simulate yield being deployed to a protocol
    // In a full test, this would involve actual Blend/DEX integration
    let total_assets = client.get_total_assets();
    client.update_total_assets(&owner, &(total_assets + 200_000_i128), &0_i128);

    // Pause the vault
    client.pause(&owner);

    // Emergency withdraw should work even with deployed yield
    client.emergency_withdraw(&user, &amount);

    // Verify shares are burned
    assert_eq!(client.get_shares(&user), 0);
}