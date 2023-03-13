use crate::*;

//used to generate a unique prefix in our storage collections (this is to avoid data collisions)
pub(crate) fn hash_account_id(account_id: &AccountId) -> CryptoHash {
    //get the default hash
    let mut hash = CryptoHash::default();
    //we hash the account ID and return it
    hash.copy_from_slice(&env::sha256(account_id.as_bytes()));
    hash
}

impl Marketplace {
    //internal method for removing a listing from the market. This returns the previously removed listing object
    pub(crate) fn internal_remove_listing(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
    ) -> Listing {
        //get the unique listing ID (contract + DELIMITER + token ID)
        let contract_and_token_id = format!("{}{}{}", &nft_contract_id, DELIMETER, token_id);
        //get the listing object by removing the unique listing ID. If there was no listing, panic
        let listing = self.listings.remove(&contract_and_token_id).expect("No listing");

        //get the set of listings for the listing's owner. If there's no listing, panic. 
        let mut by_owner_id = self.by_owner_id.get(&listing.seller).expect("No listing by_owner_id");
        //remove the unique listing ID from the set of listings
        by_owner_id.remove(&contract_and_token_id);
        
        //if the set of listings is now empty after removing the unique listing ID, we simply remove that owner from the map
        if by_owner_id.is_empty() {
            self.by_owner_id.remove(&listing.seller);
        //if the set of listings is not empty after removing, we insert the set back into the map for the owner
        } else {
            self.by_owner_id.insert(&listing.seller, &by_owner_id);
        }

        //get the set of token IDs for listing for the nft contract ID. If there's no listing, panic. 
        let mut by_nft_contract_id = self
            .by_nft_contract_id
            .get(&nft_contract_id)
            .expect("No listing by nft_contract_id");
        
        //remove the token ID from the set 
        by_nft_contract_id.remove(&token_id);
        
        //if the set is now empty after removing the token ID, we remove that nft contract ID from the map
        if by_nft_contract_id.is_empty() {
            self.by_nft_contract_id.remove(&nft_contract_id);
        //if the set is not empty after removing, we insert the set back into the map for the nft contract ID
        } else {
            self.by_nft_contract_id
                .insert(&nft_contract_id, &by_nft_contract_id);
        }

        //return the listing object
        listing
    }
}