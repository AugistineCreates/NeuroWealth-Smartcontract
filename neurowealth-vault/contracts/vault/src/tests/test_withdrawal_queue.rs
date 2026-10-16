#![cfg(test)]

use super::utils::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn assert_withdrawal_state(
    token: &TestTokenClient,
    client: &NeuroWealthVaultClient,
    contract_id: &Address,
    user: &Address,
    user_usdc: i128,
    user_shares: i128,
    total_shares: i128,
    total_assets: i128,
    vault_usdc: i128,
) {
    assert_eq!(token.balance(user), user_usdc, "user USDC balance");
    assert_eq!(client.get_shares(user), user_shares, "user shares");
    assert_eq!(client.get_total_shares(), total_shares, "TotalShares");
    assert_eq!(client.get_total_assets(), total_assets, "TotalAssets");
    assert_eq!(token.balance(contract_id), vault_usdc, "vault USDC balance");
}

#[test]
fn test_withdrawal_queue_ordering_cancellation_and_processing() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);
    let token = TestTokenClient::new(&env, &usdc_token);
    let user = Address::generate(&env);
    let deposit = 300_0000000_i128;
    mint_and_deposit(&env, &client, &usdc_token, &user, deposit);

    // Configure queue: max_size = 5, ttl = 3600s
    client.set_queue_config(&5u32, &3600u64);
    let (max_size, ttl) = client.get_queue_config();
    assert_eq!(max_size, 5);
    assert_eq!(ttl, 3600);

    // Queue withdrawal request
    let req_id1 = client.queue_withdrawal(&user, &200_0000000);
    assert_eq!(req_id1, 1);

    let req1 = client.get_withdrawal_request(&req_id1);
    assert_eq!(req1.user, user);
    assert_eq!(req1.amount, 200_0000000);
    assert!(!req1.fulfilled);
    assert!(!req1.cancelled);

    // Queue second withdrawal request
    let req_id2 = client.queue_withdrawal(&user, &100_0000000);
    assert_eq!(req_id2, 2);

    // Cancel req2
    client.cancel_withdrawal_request(&user, &req_id2);
    assert!(client.try_get_withdrawal_request(&req_id2).is_err());

    assert_withdrawal_state(
        &token,
        &client,
        &contract_id,
        &user,
        0,
        deposit,
        deposit,
        deposit,
        deposit,
    );

    // Agent processes queue in FIFO order
    let processed = client.process_withdrawal_queue(&agent, &10u32);
    assert_eq!(processed, 1); // 1 fulfilled, 1 cancelled skipped

    assert!(client.try_get_withdrawal_request(&req_id1).is_err());
    assert_withdrawal_state(
        &token,
        &client,
        &contract_id,
        &user,
        200_0000000,
        100_0000000,
        100_0000000,
        100_0000000,
        100_0000000,
    );
}

#[test]
fn test_any_authenticated_keeper_can_process_bounded_queue() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, _agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);
    let token = TestTokenClient::new(&env, &usdc_token);
    let user = Address::generate(&env);
    let keeper = soroban_sdk::Address::generate(&env);
    let deposit = 1_000_i128;
    mint_and_deposit(&env, &client, &usdc_token, &user, deposit);
    client.set_queue_config(&100u32, &3600u64);

    for _ in 0..55 {
        client.queue_withdrawal(&user, &1);
    }

    assert_withdrawal_state(
        &token,
        &client,
        &contract_id,
        &user,
        0,
        deposit,
        deposit,
        deposit,
        deposit,
    );
    assert_eq!(
        client.process_withdrawal_queue(&keeper, &u32::MAX),
        crate::MAX_WITHDRAWAL_PROCESS_BATCH,
    );
    assert_withdrawal_state(&token, &client, &contract_id, &user, 50, 950, 950, 950, 950);
    assert_eq!(client.process_withdrawal_queue(&keeper, &u32::MAX), 5);
    assert_withdrawal_state(&token, &client, &contract_id, &user, 55, 945, 945, 945, 945);
}

#[test]
fn test_withdrawal_queue_keeps_request_pending_with_partial_liquidity() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, agent, owner, usdc_token, blend_pool) =
        setup_vault_with_token_and_blend(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);
    let token = TestTokenClient::new(&env, &usdc_token);
    let user = Address::generate(&env);
    let deposit = 10_0000000_i128;

    client.set_blend_pool(&owner, &blend_pool);
    mint_and_deposit(&env, &client, &usdc_token, &user, deposit);
    client.rebalance(&soroban_sdk::symbol_short!("blend"), &500_i128, &0_i128);

    token.mint(&contract_id, &3_0000000_i128);
    client.update_total_assets(&agent, &13_0000000_i128, &false, &0_u32);
    let request_id = client.queue_withdrawal(&user, &5_0000000_i128);

    assert_withdrawal_state(
        &token,
        &client,
        &contract_id,
        &user,
        0,
        deposit,
        deposit,
        13_0000000,
        3_0000000,
    );
    assert_eq!(client.process_withdrawal_queue(&agent, &10), 0);
    assert!(client.try_get_withdrawal_request(&request_id).is_ok());
    assert_withdrawal_state(
        &token,
        &client,
        &contract_id,
        &user,
        0,
        deposit,
        deposit,
        13_0000000,
        3_0000000,
    );
}
