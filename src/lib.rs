use external::ext_contract;
use near_contract_standards::non_fungible_token::{NonFungibleToken, TokenId};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap, UnorderedSet};
use near_sdk::env::STORAGE_PRICE_PER_BYTE;
use near_sdk::json_types::{U128, U64};
use near_sdk::{
    assert_one_yocto, env, ext_contract, near_bindgen, promise_result_as_success, AccountId,
    Balance, BorshStorageKey, CryptoHash, Gas, PanicOnDefault, Promise,
};
use serde::{Deserialize, Serialize};

mod external;
mod internal;
mod nft_callback;
mod sale_views;

#[cfg(test)]
mod test;

const GAS_FOR_RESOLVE_PURCHASE: Gas = Gas(115_000_000_000_000);
const GAS_FOR_NFT_TRANSFER: Gas = Gas(15_000_000_000_000);

//the minimum storage to have a sale on the contract.
const STORAGE_PER_SALE: u128 = 1000 * STORAGE_PRICE_PER_BYTE;

//every sale will have a unique ID which is `CONTRACT + DELIMITER + TOKEN_ID`
static DELIMETER: &str = ".";

pub type ContractAndTokenId = String;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, PartialEq, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Listing {
    //owner of the sale
    pub seller: AccountId,
    //market contract's approval ID to transfer the token on behalf of the owner
    pub approval_id: u64,
    //nft contract where the token was minted
    pub nft_contract_id: String,
    //actual token ID for sale
    pub token_id: String,
    //sale price in yoctoNEAR that the token is listed for
    pub starting_price: u128,

    pub started_at: u64,

    pub end_at: u64,

    pub highest_bidder: Option<AccountId>,

    pub highest_price: u128,

    pub is_auction: bool,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Marketplace {
    pub owner: AccountId,
    pub owner_cut: u16,
    pub listings: UnorderedMap<ContractAndTokenId, Listing>,
    //keep track of the storage that accounts have payed
    pub storage_deposits: LookupMap<AccountId, Balance>,
    //keep track of all the Sale IDs for every account ID
    pub by_owner_id: LookupMap<AccountId, UnorderedSet<ContractAndTokenId>>,
    //keep track of all the token IDs for sale for a given contract
    pub by_nft_contract_id: LookupMap<AccountId, UnorderedSet<TokenId>>,
}

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKey {
    Sales,
    ByOwnerId,
    ByOwnerIdInner { account_id_hash: CryptoHash },
    ByNFTContractId,
    ByNFTContractIdInner { account_id_hash: CryptoHash },
    ByNFTTokenType,
    ByNFTTokenTypeInner { token_type_hash: CryptoHash },
    FTTokenIds,
    StorageDeposits,
}

#[near_bindgen]
impl Marketplace {
    #[init]
    pub fn new(_owner_cut: u16) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        let owner_id = env::signer_account_id();
        Self {
            owner: owner_id,
            owner_cut: _owner_cut,
            listings: UnorderedMap::new(StorageKey::Sales),
            by_owner_id: LookupMap::new(StorageKey::ByOwnerId),
            by_nft_contract_id: LookupMap::new(StorageKey::ByNFTContractId),
            storage_deposits: LookupMap::new(StorageKey::StorageDeposits),
        }
    }

    //Allows users to deposit storage. This is to cover the cost of storing sale objects on the contract
    //Optional account ID is to users can pay for storage for other people.
    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<AccountId>) {
        //get the account ID to pay for storage for
        let storage_account_id = account_id
            //convert the valid account ID into an account ID
            .map(|a| a.into())
            //if we didn't specify an account ID, we simply use the caller of the function
            .unwrap_or_else(env::predecessor_account_id);

        //get the deposit value which is how much the user wants to add to their storage
        let deposit = env::attached_deposit();

        //make sure the deposit is greater than or equal to the minimum storage for a sale
        assert!(
            deposit >= STORAGE_PER_SALE,
            "Requires minimum deposit of {}",
            STORAGE_PER_SALE
        );

        //get the balance of the account (if the account isn't in the map we default to a balance of 0)
        let mut balance: u128 = self.storage_deposits.get(&storage_account_id).unwrap_or(0);
        //add the deposit to their balance
        balance += deposit;
        //insert the balance back into the map for that account ID
        self.storage_deposits.insert(&storage_account_id, &balance);
    }

    //Allows users to withdraw any excess storage that they're not using. Say Bob pays 0.01N for 1 sale
    //Alice then buys Bob's token. This means bob has paid 0.01N for a sale that's no longer on the marketplace
    //Bob could then withdraw this 0.01N back into his account.
    #[payable]
    pub fn storage_withdraw(&mut self) {
        //make sure the user attaches exactly 1 yoctoNEAR for security purposes.
        //this will redirect them to the NEAR wallet (or requires a full access key).
        assert_one_yocto();

        //the account to withdraw storage to is always the function caller
        let owner_id = env::predecessor_account_id();
        //get the amount that the user has by removing them from the map. If they're not in the map, default to 0
        let mut amount = self.storage_deposits.remove(&owner_id).unwrap_or(0);

        //how many sales is that user taking up currently. This returns a set
        let sales = self.by_owner_id.get(&owner_id);
        //get the length of that set.
        let len = sales.map(|s| s.len()).unwrap_or_default();
        //how much NEAR is being used up for all the current sales on the account
        let diff = u128::from(len) * STORAGE_PER_SALE;

        //the excess to withdraw is the total storage paid - storage being used up.
        amount -= diff;

        //if that excess to withdraw is > 0, we transfer the amount to the user.
        if amount > 0 {
            Promise::new(owner_id.clone()).transfer(amount);
        }
        //we need to add back the storage being used up into the map if it's greater than 0.
        //this is so that if the user had 500 sales on the market, we insert that value here so
        //if those sales get taken down, the user can then go and withdraw 500 sales worth of storage.
        if diff > 0 {
            self.storage_deposits.insert(&owner_id, &diff);
        }
    }

    pub fn create_listing(
        &mut self,
        _nft_address: AccountId,
        _token_id: String,

        _starting_price: u128,
        _end_at: u64,
        _started_at: u64,
        _highest_price: u128,
        _is_auction: bool,
    ) {
        let seller = env::signer_account_id();
        let contract_and_token_id = format!("{}{}{}", _nft_address, DELIMETER, _token_id);
        assert!(
            self.listings.get(&contract_and_token_id) != None,
            "NFT not approved yet"
        );
        let mut listing = self.listings.get(&contract_and_token_id).unwrap();

        listing.seller = seller;
        listing.starting_price = _starting_price;
        listing.end_at = _end_at;
        listing.started_at = _started_at;
        listing.is_auction = _is_auction;

        self.listings.insert(&contract_and_token_id, &listing);
    }

    #[payable]
    pub fn bid(&mut self, _nft_address: AccountId, _token_id: String, _price: u128) {
        assert_one_yocto();
        let signer = env::signer_account_id();

        let contract_and_token_id = format!("{}{}{}", _nft_address, DELIMETER, _token_id);
        assert!(
            self.listings.get(&contract_and_token_id) != None,
            "NFT not listed yet"
        );
        let mut listing = self.listings.get(&contract_and_token_id).unwrap();
        assert!(listing.is_auction == true, "Not auction");
        assert!(
            Self::is_on_auction(listing.clone()) == true,
            "Auction not on"
        );
        assert!(listing.seller != signer, "Invalid bid");
        assert!(_price > listing.highest_price, "Invalid price");
        listing.highest_price = _price;
        listing.highest_bidder = Some(signer);
    }

    pub fn cancel_listing(&mut self, _nft_address: AccountId, _token_id: String) {
        let signer = env::signer_account_id();

        let contract_and_token_id = format!("{}{}{}", _nft_address, DELIMETER, _token_id);
        assert!(
            self.listings.get(&contract_and_token_id) != None,
            "NFT not listed yet"
        );
        let listing = self.listings.get(&contract_and_token_id).unwrap();
        assert!(signer == listing.seller, "Not authorized");
        self.listings.remove(&contract_and_token_id);
    }

    #[payable]
    pub fn purchase_nft(&mut self, _nft_address: AccountId, _token_id: String) {
        let signer = env::signer_account_id();
        let deposit = env::attached_deposit();

        let contract_and_token_id = format!("{}{}{}", _nft_address, DELIMETER, _token_id);
        assert!(
            self.listings.get(&contract_and_token_id) != None,
            "NFT not listed yet"
        );
        let listing = self.listings.get(&contract_and_token_id).unwrap();
        if listing.is_auction == true {
            assert!(
                Self::is_on_auction(listing.clone()) == true && listing.highest_price > 0,
                "Auction not on"
            );
            assert!(listing.highest_bidder.unwrap() == signer, "not winner");
            assert!(listing.highest_price <= deposit);
        } else {
            assert!(listing.starting_price <= deposit);
        }

        self.process_purchase(
            _nft_address,
            _token_id,
            U128(deposit),
            listing.seller,
            signer,
        );
    }

    pub fn set_price(&mut self, _nft_address: AccountId, _token_id: String, _price: u128) {
        let signer = env::signer_account_id();

        let contract_and_token_id = format!("{}{}{}", _nft_address, DELIMETER, _token_id);
        assert!(
            self.listings.get(&contract_and_token_id) != None,
            "NFT not listed yet"
        );
        let mut listing = self.listings.get(&contract_and_token_id).unwrap();
        assert!(listing.is_auction == false, "is auction");
        assert!(signer == listing.seller, "Not authorized");
        listing.starting_price = _price;

        self.listings.insert(&contract_and_token_id, &listing);
    }

    pub fn storage_minimum_balance(&self) -> U128 {
        U128(STORAGE_PER_SALE)
    }

    pub fn storage_balance_of(&self, account_id: AccountId) -> U128 {
        U128(self.storage_deposits.get(&account_id).unwrap_or(0))
    }

    fn is_on_auction(listing: Listing) -> bool {
        return env::block_timestamp() > listing.started_at
            && env::block_timestamp() < listing.end_at;
    }

    #[private]
    pub fn process_purchase(
        &mut self,
        nft_contract_id: AccountId,
        token_id: String,
        price: U128,
        seller: AccountId,
        buyer: AccountId,
    ) -> Promise {
        //get the sale object by removing the sale
        let sale =
            self.internal_remove_listing(nft_contract_id.clone(), token_id.to_string().clone());

        //a payout object used for the market to distribute funds to the appropriate accounts.
        ext_contract::ext(nft_contract_id)
            // Attach 1 yoctoNEAR with static GAS equal to the GAS for nft transfer. Also attach an unused GAS weight of 1 by default.
            .with_attached_deposit(1)
            .with_static_gas(GAS_FOR_NFT_TRANSFER)
            .nft_transfer_payout(
                buyer.clone(),                    //purchaser (person to transfer the NFT to)
                token_id.to_string(),             //token ID to transfer
                sale.approval_id, //market contract's approval ID in order to transfer the token on behalf of the owner
                "payout from market".to_string(), //memo (to include some context)
                /*
                    the price that the token was purchased for. This will be used in conjunction with the royalty percentages
                    for the token in order to determine how much money should go to which account.
                */
                price,
                10, //the maximum amount of accounts the market can payout at once (this is limited by GAS)
            )
            //after the transfer payout has been initiated, we resolve the promise by calling our own resolve_purchase function.
            //resolve purchase will take the payout object returned from the nft_transfer_payout and actually pay the accounts
            .then(
                // No attached deposit with static GAS equal to the GAS for resolving the purchase. Also attach an unused GAS weight of 1 by default.
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_PURCHASE)
                    .resolve_purchase(seller, price.into()),
            )
    }

    #[private]
    pub fn resolve_purchase(&mut self, seller: AccountId, price: u128) -> u128 {
        let owner_cut = price
            .saturating_mul(self.owner_cut.into())
            .saturating_div(10000);

        // NEAR payouts
        Promise::new(seller).transfer(price.saturating_sub(owner_cut));
        Promise::new(self.owner.clone()).transfer(owner_cut);

        //return the price payout out
        price
    }
}

#[ext_contract(ext_self)]
trait ExtSelf {
    fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        price: U128,
    ) -> Promise;
}