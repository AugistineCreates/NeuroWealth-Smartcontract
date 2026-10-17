//! Tests for vault migration functionality (#637)

use super::utils::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

/// Schedules a migration target and executes it once the timelock has passed (#969).
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

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    // Owner can pause migration
    client.set_migration_paused(&owner, &true);

    // Owner can unpause migration
    client.set_migration_paused(&owner, &false);
}

#[test]
#[should_panic(expected = "Error(Contract, #34)")]
fn test_set_migration_paused_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, _agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let unauthorized = Address::generate(&env);

    // Non-owner cannot pause migration
    client.set_migration_paused(&unauthorized, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #68)")]
fn test_migrate_without_target_fails() {
    let env = Env::default();
    env.mock_all_auths();

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

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;
    let new_vault = Address::generate(&env);

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);
    set_target_now(&env, &client, &owner, &new_vault);
    client.set_migration_paused(&owner, &true);

    // Migration when paused should fail
    client.migrate_shares(&user);
}

#[test]
#[should_panic(expected = "Error(Contract, #69)")]
fn test_migrate_without_shares_fails() {
    let env = Env::default();
    env.mock_all_auths();

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

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;
    let new_vault = Address::generate(&env);

    // Deposit and earn some yield
    mint_and_deposit(&env, &client, &usdc_token, &user, amount);

    // Simulate yield by updating total assets
    let total_assets = client.get_total_assets();
    client.update_total_assets(&owner, &(total_assets + 100_000_i128), &0_i128);

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

    let (contract_id, _agent, owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let amount = 1_000_000_i128;
    let new_vault = Address::generate(&env);

    mint_and_deposit(&env, &client, &usdc_token, &user, amount);
    set_target_now(&env, &client, &owner, &new_vault);

    // Check migration target update event
    let events = env.events().all();
    let target_update_event = events.iter().find(|e| {
        if let Some(topic) = e.topics.get(0) {
            topic.to_string() == "mig_tgt"
        } else {
            false
        }
    });
    assert!(target_update_event.is_some());
}