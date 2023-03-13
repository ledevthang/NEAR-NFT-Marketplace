use crate::*;
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]

const MIN_REQUIRED_APPROVAL_YOCTO: u128 = 170000000000000000000;
const MIN_REQUIRED_STORAGE_YOCTO: u128 = 10000000000000000000000;

mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    use super::*;

    // Allows for modifying the environment of the mocked blockchain
    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    #[test]
    #[should_panic(expected = "Requires minimum deposit of 10000000000000000000000")]
    fn test_storage_deposit_insufficient_deposit() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Marketplace::new(10);
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MIN_REQUIRED_APPROVAL_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.storage_deposit(Some(accounts(0)));
    }

    #[test]
    fn test_storage_deposit() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Marketplace::new(10);
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MIN_REQUIRED_STORAGE_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.storage_deposit(Some(accounts(0)));
        let outcome = contract.storage_deposits.get(&accounts(0));
        let expected = MIN_REQUIRED_STORAGE_YOCTO;
        assert_eq!(outcome, Some(expected));
    }

    #[test]
    fn test_storage_balance_of() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Marketplace::new(10);
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MIN_REQUIRED_STORAGE_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.storage_deposit(Some(accounts(0)));
        let balance = contract.storage_balance_of(accounts(0));
        assert_eq!(balance, U128(MIN_REQUIRED_STORAGE_YOCTO));
    }

    #[test]
    fn test_storage_withdraw() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Marketplace::new(10);

        // deposit amount
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MIN_REQUIRED_STORAGE_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.storage_deposit(Some(accounts(0)));

        // withdraw amount
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(U128(1).0) // below func requires a min of 1 yocto attached
            .predecessor_account_id(accounts(0))
            .build());
        contract.storage_withdraw();

        let remaining_amount = contract.storage_balance_of(accounts(0));
        assert_eq!(remaining_amount, U128(0))
    }

    #[test]
    fn test_remove_sale() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Marketplace::new(10);

        // deposit amount
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MIN_REQUIRED_STORAGE_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.storage_deposit(Some(accounts(0)));

        // add sale
        let token_id = String::from("0n3C0ntr4ctT0Rul3Th3m4ll");
        let sale = Listing {
            seller: accounts(0).clone(), //owner of the sale / token
            approval_id: U64(1).0,       //approval ID for that token that was given to the market
            nft_contract_id: env::predecessor_account_id().to_string(), //NFT contract the token was minted on
            token_id: token_id.clone(),                                 //the actual token ID

            starting_price: 0,
            end_at: 0,
            started_at: 0,
            highest_bidder: None,
            highest_price: 0,
            is_auction: false,
        };
        let nft_contract_id = env::predecessor_account_id();
        let contract_and_token_id = format!("{}{}{}", nft_contract_id, DELIMETER, token_id);
        contract.listings.insert(&contract_and_token_id, &sale);
        let owner_token_set = UnorderedSet::new(contract_and_token_id.as_bytes());
        contract.by_owner_id.insert(&sale.seller, &owner_token_set);
        let nft_token_set = UnorderedSet::new(token_id.as_bytes());
        contract
            .by_nft_contract_id
            .insert(&sale.seller, &nft_token_set);
        assert_eq!(
            contract.listings.len(),
            1,
            "Failed to insert sale to contract"
        );

        // remove sale
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(U128(1).0) // below func requires a min of 1 yocto attached
            .predecessor_account_id(accounts(0))
            .build());
        contract.cancel_listing(nft_contract_id, token_id);
        assert_eq!(
            contract.listings.len(),
            0,
            "Failed to remove sale from contract"
        );
    }

    #[test]
    fn test_update_price() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Marketplace::new(10);

        // deposit amount
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(MIN_REQUIRED_STORAGE_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.storage_deposit(Some(accounts(0)));

        // add sale
        let token_id = String::from("0n3C0ntr4ctT0Rul3Th3m4ll");
        let nft_bid_yocto = U128(100);
        let sale = Listing {
            seller: accounts(0).clone(), //owner of the sale / token
            approval_id: U64(1).0,       //approval ID for that token that was given to the market
            nft_contract_id: env::predecessor_account_id().to_string(), //NFT contract the token was minted on
            token_id: token_id.clone(),                                 //the actual token ID

            starting_price: 0,
            end_at: 0,
            started_at: 0,
            highest_bidder: None,
            highest_price: 0,
            is_auction: false,
        };
        let nft_contract_id = env::predecessor_account_id();
        let contract_and_token_id = format!("{}{}{}", nft_contract_id, DELIMETER, token_id);
        contract.listings.insert(&contract_and_token_id, &sale);
        let owner_token_set = UnorderedSet::new(contract_and_token_id.as_bytes());
        contract.by_owner_id.insert(&sale.seller, &owner_token_set);
        let nft_token_set = UnorderedSet::new(token_id.as_bytes());
        contract
            .by_nft_contract_id
            .insert(&sale.seller, &nft_token_set);
        assert_eq!(
            contract.listings.len(),
            1,
            "Failed to insert sale to contract"
        );

        // update price
        let new_price = U128(150);
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(U128(1).0)
            .predecessor_account_id(accounts(0)) // bob to buy NFT from alice
            .build());
        contract.set_price(nft_contract_id.clone(), token_id.clone(), new_price.into());

        // test update price success
        let sale = contract
            .listings
            .get(&contract_and_token_id)
            .expect("No sale");
        assert_eq!(sale.starting_price, new_price.into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(new_price.into())
            .predecessor_account_id(accounts(0))
            .build());
        contract.purchase_nft(nft_contract_id, token_id);
        
    }
}
