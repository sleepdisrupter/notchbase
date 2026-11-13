mod columns;
mod rebuild;
mod shape;

use super::{flag, tier, with_tier};

#[test]
fn tier_lives_in_two_bits() {
    let hidden_folder = flag::DIR | flag::HIDDEN;
    for level in 0..4 {
        let flags = with_tier(hidden_folder, level);
        assert_eq!(tier(flags), level);
        assert_eq!(flags & !flag::TIER_MASK, hidden_folder);
    }
    assert_eq!(with_tier(with_tier(flag::CLOUD, 3), 1), with_tier(flag::CLOUD, 1));
}

#[test]
fn fixed_bits_never_overlap() {
    let every = [flag::DIR, flag::HIDDEN, flag::TIER_MASK, flag::INSIDE_PACKAGE, flag::CLOUD, flag::LINK, flag::EXOTIC];
    assert_eq!(every.iter().fold(0u8, |all, bit| all | bit), u8::MAX);
    assert_eq!(every.iter().map(|bit| bit.count_ones()).sum::<u32>(), 8, "no two flags share a bit");
    let may_change = flag::HIDDEN | flag::TIER_MASK | flag::INSIDE_PACKAGE | flag::CLOUD;
    assert_eq!(flag::IDENTITY & may_change, 0);
    assert_eq!(flag::IDENTITY | may_change | flag::EXOTIC, u8::MAX);
}
