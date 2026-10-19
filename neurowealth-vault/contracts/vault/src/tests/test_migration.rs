//! Tests for vault migration functionality (#637)

use super::utils::*;
use crate::TOPIC_MIGRATION_TARGET_UPDATED;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

/// Schedules a migration target and executes it once the timelock has passed (#969).
///
/// Keeps instance storage alive across the ~17,280-ledger jump the timelock
/// requires -- without this, the vault's own contract instance entry can be
/// archived by the time `execute_migration_target` runs, failing with an
/// unrelated "accessed archived contract instance" error. Must be called
/// before the vault under test is deployed/initialized, since the TTL a
/// contract's instance entry gets is fixed at creation time.
fn extend_ttls_for_migration_timelock(env: &Env) {
    env.ledger().with_mut(|li| {
        li.min_persistent_entry_ttl = 100_000;
        li.min_temp_entry_ttl = 100_000;
        li.max_entry_ttl = 200_000;
    });
}

fn set_target_now(env: &Env, client: &NeuroWealthVaultClient, owner: &Address, target: &Address) {
    client.schedule_migration_target(owner, target);
    let (_, effective_ledger) = client.get_pending_migration_target().unwrap();
    env.ledger().set_sequence_number(effective_ledger);
    client.execute_migration_target(owner);
}

#[test]
fn test_set_migration_target_by_owner() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let new_vault = Address::generate(&env);

    // Owner can set migration target
    set_target_now(&env, &client, &owner, &new_vault);

    // Verify the target was set
    // Note: We'd need a getter for migration target, but for now test the event
    let events = env.events().all();
    assert!(events.len() > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #34)")]
fn test_set_migration_target_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let unauthorized = Address::generate(&env);
    let new_vault = Address::generate(&env);

    // Non-owner cannot set migration target
    client.schedule_migration_target(&unauthorized, &new_vault);
}

#[test]
fn test_set_migration_paused() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    // Owner can pause migration
    client.set_migration_paused(&true);

    // Owner can unpause migration
    client.set_migration_paused(&false);
}

/// `set_migration_paused` enforces ownership via `require_is_owner`, which
/// calls `owner.require_auth()` on the stored owner address with no
/// separate caller parameter, so the guard is tested by revoking all auth
/// and checking the `try_*` variant is rejected.
#[test]
fn test_set_migration_paused_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, _owner, _usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    env.mock_auths(&[]);

    let result = client.try_set_migration_paused(&true);
    assert!(
        result.is_err(),
        "set_migration_paused must reject calls without the owner's authorization"
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #68)")]
fn test_migrate_without_target_fails() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Migration without target should fail
    client.migrate_shares(&user);
}

#[test]
#[should_panic(expected = "Error(Contract, #67)")]
fn test_migrate_when_paused_fails() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;
    let new_vault = Address::generate(&env);

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);
    set_target_now(&env, &client, &owner, &new_vault);
    client.set_migration_paused(&true);

    // Migration when paused should fail
    client.migrate_shares(&user);
}

#[test]
#[should_panic(expected = "Error(Contract, #69)")]
fn test_migrate_without_shares_fails() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let new_vault = Address::generate(&env);

    set_target_now(&env, &client, &owner, &new_vault);

    // Migration without shares should fail
    client.migrate_shares(&user);
}

#[test]
fn test_exchange_rate_preserved_during_migration() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;
    let new_vault = Address::generate(&env);

    // Deposit and earn some yield
    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Simulate yield landing in the vault. `update_total_assets` requires
    // the vault's actual token balance to cover the reported total, and is
    // called by the agent, not the owner.
    let yield_amount = 100_000_i128;
    let token_client = TestTokenClient::new(&env, &usdc_token);
    token_client.mint(&contract_id, &yield_amount);
    let total_assets = client.get_total_assets();
    client.update_total_assets(&agent, &(total_assets + yield_amount), &false, &0_u32);

    // Calculate expected exchange rate before migration
    let shares_before = client.get_shares(&user);
    let total_assets_before = client.get_total_assets();
    let total_shares_before = client.get_total_shares();
    let expected_value = (shares_before * total_assets_before) / total_shares_before;

    // Set migration target
    set_target_now(&env, &client, &owner, &new_vault);

    // Note: Full migration test would require setting up a second vault contract
    // For now, we test the setup and state changes
    assert!(shares_before > 0);
    assert!(total_assets_before > amount); // Yield accrued
}

#[test]
fn test_migration_emits_correct_event() {
    let env = Env::default();
    env.mock_all_auths();
    extend_ttls_for_migration_timelock(&env);

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;
    let new_vault = Address::generate(&env);

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);
    set_target_now(&env, &client, &owner, &new_vault);

    // Check migration target update event
    let matching = find_events_by_topic(env.events().all(), &env, TOPIC_MIGRATION_TARGET_UPDATED);
    assert!(
        !matching.is_empty(),
        "execute_migration_target must publish a TOPIC_MIGRATION_TARGET_UPDATED event"
    );
}
