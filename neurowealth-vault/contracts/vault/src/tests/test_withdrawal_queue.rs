#![cfg(test)]

use crate::tests::utils::setup_vault;

#[test]
fn test_withdrawal_queue_ordering_cancellation_and_processing() {
    let (env, client, owner, agent, user, usdc_token, _) = setup_vault();

    // Configure queue: max_size = 5, ttl = 3600s
    client.set_queue_config(&owner, &5u32, &3600u64).unwrap();
    let (max_size, ttl) = client.get_queue_config();
    assert_eq!(max_size, 5);
    assert_eq!(ttl, 3600);

    // Deposit to establish vault state
    let deposit_amount = 1_000_0000000i128; // 1000 USDC
    soroban_sdk::token::Client::new(&env, &usdc_token).approve(
        &user,
        &soroban_sdk::Address::from_contract_id(&env, &client.address),
        &deposit_amount,
        &200,
    );
    client.deposit(&user, &deposit_amount);

    // Queue withdrawal request
    let req_id1 = client.queue_withdrawal(&user, &200_0000000);
    assert_eq!(req_id1, 1);

    let req1 = client.get_withdrawal_request(&req_id1).unwrap();
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

    // Agent processes queue in FIFO order
    let user_balance_before = soroban_sdk::token::Client::new(&env, &usdc_token).balance(&user);
    let processed = client.process_withdrawal_queue(&agent, &10u32);
    assert_eq!(processed, 1); // 1 fulfilled, 1 cancelled skipped
    let user_balance_after = soroban_sdk::token::Client::new(&env, &usdc_token).balance(&user);

    // Verify user received USDC
    assert!(user_balance_after > user_balance_before);
    assert!(client.try_get_withdrawal_request(&req_id1).is_err());
}

#[test]
fn test_any_authenticated_keeper_can_process_bounded_queue() {
    let (env, client, owner, _agent, user, usdc_token, _) = setup_vault();
    let keeper = soroban_sdk::Address::generate(&env);
    client.set_queue_config(&owner, &100u32, &3600u64).unwrap();

    // Deposit to establish vault state
    let deposit_amount = 100_000_0000000i128; // 100k USDC
    soroban_sdk::token::Client::new(&env, &usdc_token).approve(
        &user,
        &soroban_sdk::Address::from_contract_id(&env, &client.address),
        &deposit_amount,
        &200,
    );
    client.deposit(&user, &deposit_amount);

    for _ in 0..55 {
        client.queue_withdrawal(&user, &1);
    }

    assert_eq!(
        client.process_withdrawal_queue(&keeper, &u32::MAX),
        crate::MAX_WITHDRAWAL_PROCESS_BATCH,
    );
    assert_eq!(client.process_withdrawal_queue(&user, &u32::MAX), 5);
}

#[test]
fn test_cannot_over_queue_withdrawals() {
    let (env, client, owner, _agent, user, usdc_token, _) = setup_vault();
    client.set_queue_config(&owner, &100u32, &3600u64).unwrap();

    // Deposit 1000 USDC
    let deposit_amount = 1_000_0000000i128;
    soroban_sdk::token::Client::new(&env, &usdc_token).approve(
        &user,
        &soroban_sdk::Address::from_contract_id(&env, &client.address),
        &deposit_amount,
        &200,
    );
    client.deposit(&user, &deposit_amount);

    // Queue 900 USDC
    let req_id1 = client.queue_withdrawal(&user, &900_0000000);
    assert_eq!(req_id1, 1);

    // Try to queue another 900 USDC (should fail - only 100 available)
    assert!(client.try_queue_withdrawal(&user, &900_0000000).is_err());

    // Should be able to queue 100 USDC (the remaining amount)
    let req_id2 = client.queue_withdrawal(&user, &100_0000000);
    assert_eq!(req_id2, 2);

    // Now all shares are reserved, any additional queue should fail
    assert!(client.try_queue_withdrawal(&user, &1).is_err());
}

#[test]
fn test_cancel_unreserves_shares() {
    let (env, client, owner, _agent, user, usdc_token, _) = setup_vault();
    client.set_queue_config(&owner, &100u32, &3600u64).unwrap();

    // Deposit 1000 USDC
    let deposit_amount = 1_000_0000000i128;
    soroban_sdk::token::Client::new(&env, &usdc_token).approve(
        &user,
        &soroban_sdk::Address::from_contract_id(&env, &client.address),
        &deposit_amount,
        &200,
    );
    client.deposit(&user, &deposit_amount);

    // Queue 900 USDC
    let req_id1 = client.queue_withdrawal(&user, &900_0000000);
    
    // Should not be able to withdraw/queue more
    assert!(client.try_queue_withdrawal(&user, &101_0000000).is_err());

    // Cancel the request
    client.cancel_withdrawal_request(&user, &req_id1);

    // Now should be able to queue again
    let req_id2 = client.queue_withdrawal(&user, &900_0000000);
    assert_eq!(req_id2, 2);
}

#[test]
fn test_cannot_queue_locked_shares() {
    let (env, client, owner, _agent, user, usdc_token, _) = setup_vault();
    client.set_queue_config(&owner, &100u32, &3600u64).unwrap();

    // Deposit 1000 USDC
    let deposit_amount = 1_000_0000000i128;
    soroban_sdk::token::Client::new(&env, &usdc_token).approve(
        &user,
        &soroban_sdk::Address::from_contract_id(&env, &client.address),
        &deposit_amount,
        &200,
    );
    client.deposit(&user, &deposit_amount);

    // Lock shares (this would be done by a lock mechanism, but for now we'll assume it's set)
    // Lock all shares via owner
    client.lock_shares(&user, &(1_000_0000000i128 / 2), &1000u32, &owner).ok();

    // Try to queue all shares (should fail - half are locked)
    assert!(client.try_queue_withdrawal(&user, &1_000_0000000).is_err());

    // Should be able to queue only unlocked amount
    let req_id = client.queue_withdrawal(&user, &500_0000000);
    assert!(req_id > 0);
}
