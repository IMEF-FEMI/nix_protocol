mod free_addr_helpers {

    use crate::state::market::{MarketFixed, MarketUnusedFreeListPadding};
    use hypertree::{DataIndex, FreeList};

    pub fn get_free_address_on_market_fixed(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
    ) -> DataIndex {
        let mut free_list: FreeList<MarketUnusedFreeListPadding> =
            FreeList::new(dynamic, fixed.free_list_head_index);
        let free_address: DataIndex = free_list.remove();
        fixed.free_list_head_index = free_list.get_head();
        free_address
    }

    pub fn get_free_address_on_market_fixed_for_seat(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
    ) -> DataIndex {
        get_free_address_on_market_fixed(fixed, dynamic)
    }

    pub fn release_address_on_market_fixed(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
        index: DataIndex,
    ) {
        let mut free_list: FreeList<MarketUnusedFreeListPadding> =
            FreeList::new(dynamic, fixed.free_list_head_index);
        free_list.add(index);
        fixed.free_list_head_index = index;
    }
    pub fn get_free_address_on_market_fixed_for_bid_order(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
    ) -> DataIndex {
        get_free_address_on_market_fixed(fixed, dynamic)
    }

    pub fn get_free_address_on_market_fixed_for_ask_order(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
    ) -> DataIndex {
        get_free_address_on_market_fixed(fixed, dynamic)
    }
    
    pub fn release_address_on_market_fixed_for_seat(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
        index: DataIndex,
    ) {
        release_address_on_market_fixed(fixed, dynamic, index);
    }

    pub fn release_address_on_market_fixed_for_bid_order(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
        index: DataIndex,
    ) {
        release_address_on_market_fixed(fixed, dynamic, index);
    }

    pub fn release_address_on_market_fixed_for_ask_order(
        fixed: &mut MarketFixed,
        dynamic: &mut [u8],
        index: DataIndex,
    ) {
        release_address_on_market_fixed(fixed, dynamic, index);
    }
}
pub use free_addr_helpers::*;
