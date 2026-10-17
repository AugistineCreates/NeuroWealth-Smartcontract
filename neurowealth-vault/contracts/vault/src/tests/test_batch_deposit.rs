#![cfg(test)]

use super::utils::{setup_vault_with_token, NeuroWealthVaultClient, TestTokenClient};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

#[test]
fn test_batch_deposit_single_user_multiple_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, _agent, _owner, usdc_token) = setup_vault_with_token(&env);
    let client = NeuroWealthVaultClient::new(&env, &contract_id);
    let token_client = TestTokenClient::new(&env, &usdc_token);
    let user = Address::generate(&env);
    let first_amount = 100_0000000_i128;
    let second_amount = 200_0000000_i128;
    token_client.mint(&user, &(first_amount + second_amount));

    let entries = vec![
        &env,
        (usdc_token.clone(), first_amount),
        (usdc_token.clone(), second_amount),
    ];
    client.batch_deposit(&user, &entries);

    assert_eq!(client.get_shares(&user), first_amount + second_amount);
    assert_eq!(token_client.balance(&contract_id), first_amount + second_amount);
}
