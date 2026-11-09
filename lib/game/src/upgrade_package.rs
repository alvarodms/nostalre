//! Upgrade packages (kRO's random upgrade, "Laphine upgrade"): an item, such as the Booster Weapon Upgrade Package,
//! that upgrades one item of the bag. Using it runs its script's `laphine_upgrade()`; the server opens the window
//! (ZC_RANDOM_UPGRADE_ITEM_UI_OPEN, 0x0ab4) saying only which package it was, and waits for the item
//! (CZ_REQ_RANDOM_UPGRADE_ITEM, 0x0ab6). It checks the item, spends one package, sets the refine and rolls the
//! options its `laphine_upgrade.yml` says, sends the item back and answers (0x0ab7). Until the window closes
//! (0x0ab5) the player can do nothing else that `pc_cant_act` guards: talk to NPCs, trade, use most windows.
//!
//! kRO's client knows from its own table (`LapineUpgradeBox.lub`) which items a package takes. These come from the
//! server's data instead (`upgrade_tables`, generated), so the window offers what the server accepts, for the
//! reasons `clif_parse_laphine_upgrade` turns an item down.

use crate::character::Character;
use crate::data_table::DataTable;
use crate::item::Item;
use crate::upgrade_tables::upgrade_rule;

/// rAthena's MAX_REFINE for Renewal: where a result with no maximum ends
pub const MAX_REFINE: u8 = 20;

/// What a package takes and what it does, as the server's laphine_upgrade.yml says
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeRule {
    /// The items it upgrades, in id order
    pub targets: &'static [u32],
    /// The refine an item needs, at least and at most
    pub min_refine: u8,
    pub max_refine: u8,
    /// Random options the item must already have
    pub required_options: u8,
    /// The refine it sets, when the item is lower (`ResultRefine`); 0 for none
    pub result_refine: u8,
    /// The range a random refine is drawn from (`ResultRefineMinimum`, `ResultRefineMaximum`); 0 for unset
    pub result_min: u8,
    pub result_max: u8,
    /// Whether an item with cards (or a maker's or pet's mark, anything in its card slots) is taken
    pub cards_allowed: bool,
    /// Whether it rolls new random options onto the item
    pub random_options: bool,
}

/// The refine an item comes back with
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefineResult {
    /// The package leaves the refine as it is
    Same,
    /// This refine
    Exactly(u8),
    /// One drawn from this range, both ends included
    Between(u8, u8),
}

/// Why the server would turn an item down, in the order it checks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unfit {
    /// Not one of the package's items
    NotTaken,
    Unidentified,
    Equipped,
    /// In the swap equipment set
    InSwapSet,
    /// Under the refine it needs
    RefineTooLow(u8),
    /// Over the refine it may have
    RefineTooHigh(u8),
    /// Something in its card slots, which the package does not take
    HasCards,
    /// Fewer random options than it needs
    TooFewOptions(u8),
}

impl Unfit {
    /// What the player is told
    pub fn reason(self) -> String {
        match self {
            Unfit::NotTaken => "This package does not take it".into(),
            Unfit::Unidentified => "Not appraised".into(),
            Unfit::Equipped => "Take it off first".into(),
            Unfit::InSwapSet => "Take it out of the swap set first".into(),
            Unfit::RefineTooLow(min) => format!("Needs +{min} or more"),
            Unfit::RefineTooHigh(max) => format!("Only up to +{max}"),
            Unfit::HasCards => "Has a card or enchant".into(),
            Unfit::TooFewOptions(n) => format!("Needs {n} random options"),
        }
    }
}

impl UpgradeRule {
    pub fn takes(&self, item_id: u32) -> bool {
        self.targets.binary_search(&item_id).is_ok()
    }

    /// Whether the server would upgrade `item` (`in_swap_set`: it is in the swap equipment set), checked as
    /// `clif_parse_laphine_upgrade` does
    pub fn check(&self, item: &Item, in_swap_set: bool) -> Result<(), Unfit> {
        if !self.takes(item.item_id) {
            return Err(Unfit::NotTaken);
        }
        if !item.is_identified {
            return Err(Unfit::Unidentified);
        }
        if item.is_equipped() {
            return Err(Unfit::Equipped);
        }
        if in_swap_set {
            return Err(Unfit::InSwapSet);
        }
        if item.refining_level < self.min_refine {
            return Err(Unfit::RefineTooLow(self.min_refine));
        }
        if item.refining_level > self.max_refine {
            return Err(Unfit::RefineTooHigh(self.max_refine));
        }
        if !self.cards_allowed && item.slot.iter().any(|&c| c != 0) {
            return Err(Unfit::HasCards);
        }
        // the server counts up to the last option set, as the options are packed from the first
        let options = item
            .options
            .iter()
            .rposition(|o| o.id != 0)
            .map_or(0, |i| i + 1);
        if options < usize::from(self.required_options) {
            return Err(Unfit::TooFewOptions(self.required_options));
        }
        Ok(())
    }

    /// The refine an item at `refine` comes back with, as the server sets it
    pub fn result(&self, refine: u8) -> RefineResult {
        if self.result_refine > 0 {
            // never lowered
            return RefineResult::Exactly(refine.max(self.result_refine));
        }
        let (low, high) = if self.result_max > 0 {
            // with a minimum it may come out lower; without, it only goes up to the maximum
            let low = if self.result_min > 0 {
                self.result_min
            } else {
                refine
            };
            (low, self.result_max)
        } else if self.result_min > 0 {
            (self.result_min, MAX_REFINE)
        } else {
            return RefineResult::Same;
        };
        let (low, high) = (low.min(high), low.max(high));
        if low == high {
            RefineResult::Exactly(low)
        } else {
            RefineResult::Between(low, high)
        }
    }

    /// What the package needs, as the window's plate shows it: "+0 ~ +12"
    pub fn requirement(&self) -> String {
        let mut text = if self.min_refine == 0 && self.max_refine >= MAX_REFINE {
            "Any refine".to_string()
        } else if self.min_refine == self.max_refine {
            format!("+{}", self.min_refine)
        } else {
            format!("+{} ~ +{}", self.min_refine, self.max_refine)
        };
        if !self.cards_allowed {
            text.push_str(" · no cards");
        }
        if self.required_options > 0 {
            text.push_str(&format!(" · {}+ options", self.required_options));
        }
        text
    }

    /// What it does to an item at `refine`: "+9 → +13", "+0 → +4 ~ +9 · new random options"
    pub fn outcome(&self, refine: u8) -> String {
        let mut text = match self.result(refine) {
            RefineResult::Same => String::new(),
            RefineResult::Exactly(r) => format!("+{refine} → +{r}"),
            RefineResult::Between(low, high) => format!("+{refine} → +{low} ~ +{high}"),
        };
        if self.random_options {
            if !text.is_empty() {
                text.push_str(" · ");
            }
            text.push_str("new random options");
        }
        if text.is_empty() {
            text.push_str("No change");
        }
        text
    }
}

/// Whether the item is an upgrade package the server knows
pub fn is_upgrade_package(item_id: u32) -> bool {
    upgrade_rule(item_id).is_some()
}

/// One of the package's items in the bag, and whether the server would take it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Its inventory index, as the client numbers it
    pub index: u16,
    pub fit: Result<(), Unfit>,
}

/// An upgrade window the server opened: the package, and what it takes and does
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradePackage {
    pub package_id: u32,
    /// The package's name, the window's title
    pub title: String,
    pub rule: &'static UpgradeRule,
}

impl UpgradePackage {
    /// The window for `package_id`, `None` when the server's table has no such package
    pub fn new(package_id: u32, data: &DataTable) -> Option<Self> {
        Some(Self {
            package_id,
            title: data.item_name_for(package_id, true),
            rule: upgrade_rule(package_id)?,
        })
    }

    /// The bag's items the package takes, the ones the server would upgrade first, each group in bag order
    pub fn candidates(&self, character: &Character) -> Vec<Candidate> {
        let mut found: Vec<Candidate> = character
            .inventory
            .all_items()
            .iter()
            .filter(|i| i.count > 0 && self.rule.takes(i.item_id))
            .map(|i| Candidate {
                index: i.index,
                fit: self.rule.check(i, character.equip_switch.contains(i.index)),
            })
            .collect();
        found.sort_by_key(|c| (c.fit.is_err(), c.index));
        found
    }

    /// Whether the bag's item at `index` is one the server would upgrade now
    pub fn fits(&self, character: &Character, index: u16) -> Result<(), Unfit> {
        let item = character.inventory.get_item(index).ok_or(Unfit::NotTaken)?;
        self.rule
            .check(item, character.equip_switch.contains(index))
    }

    /// Whether a package is still in the bag: the server spends one from wherever it finds it
    pub fn still_held(&self, character: &Character) -> bool {
        character
            .inventory
            .all_items()
            .iter()
            .any(|i| i.item_id == self.package_id && i.count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemOption, ItemType};

    /// Booster Weapon Upgrade Package: up to +12, cards kept, to +13
    const BOOSTER_WEAPON: u32 = 100340;
    /// One of its Booster weapons
    const SWORD: u32 = 500015;

    fn weapon(refine: u8) -> Item {
        let mut item = Item::from_tables(SWORD, ItemType::Weapon, &DataTable::new());
        item.index = 4;
        item.refining_level = refine;
        item
    }

    fn rule(f: impl FnOnce(&mut UpgradeRule)) -> UpgradeRule {
        let mut r = UpgradeRule {
            targets: &[SWORD],
            min_refine: 0,
            max_refine: MAX_REFINE,
            required_options: 0,
            result_refine: 0,
            result_min: 0,
            result_max: 0,
            cards_allowed: false,
            random_options: false,
        };
        f(&mut r);
        r
    }

    #[test]
    fn the_booster_weapon_package_takes_booster_weapons_up_to_plus_twelve() {
        let r = upgrade_rule(BOOSTER_WEAPON).expect("in laphine_upgrade.yml");
        assert!(r.takes(SWORD));
        assert!(!r.takes(1101));
        assert_eq!((r.min_refine, r.max_refine, r.cards_allowed), (0, 12, true));
        assert_eq!(r.result(9), RefineResult::Exactly(13));
        assert_eq!(r.outcome(9), "+9 → +13");
        assert_eq!(r.requirement(), "+0 ~ +12");
        assert!(is_upgrade_package(BOOSTER_WEAPON));
        assert!(!is_upgrade_package(501));
    }

    #[test]
    fn an_item_is_turned_down_for_each_of_the_servers_reasons() {
        let r = rule(|r| {
            r.min_refine = 4;
            r.max_refine = 9;
            r.required_options = 2;
        });
        let mut item = weapon(7);
        item.options[0] = ItemOption {
            id: 1,
            value: 5,
            param: 0,
        };
        item.options[1] = ItemOption {
            id: 2,
            value: 5,
            param: 0,
        };
        assert_eq!(r.check(&item, false), Ok(()));

        let mut other = item.clone();
        other.item_id = 1101;
        assert_eq!(r.check(&other, false), Err(Unfit::NotTaken));
        let mut unknown = item.clone();
        unknown.is_identified = false;
        assert_eq!(r.check(&unknown, false), Err(Unfit::Unidentified));
        let mut worn = item.clone();
        worn.wear_state = 2;
        assert_eq!(r.check(&worn, false), Err(Unfit::Equipped));
        assert_eq!(r.check(&item, true), Err(Unfit::InSwapSet));
        assert_eq!(r.check(&weapon(3), false), Err(Unfit::RefineTooLow(4)));
        let mut high = item.clone();
        high.refining_level = 10;
        assert_eq!(r.check(&high, false), Err(Unfit::RefineTooHigh(9)));
        let mut carded = item.clone();
        carded.slot[2] = 4001;
        assert_eq!(r.check(&carded, false), Err(Unfit::HasCards));
        let mut one = item.clone();
        one.options[1] = ItemOption::default();
        assert_eq!(r.check(&one, false), Err(Unfit::TooFewOptions(2)));
        // the server counts up to the last option set
        let mut gap = one.clone();
        gap.options[2] = ItemOption {
            id: 3,
            value: 1,
            param: 0,
        };
        assert_eq!(r.check(&gap, false), Ok(()));
    }

    #[test]
    fn the_result_refine_follows_the_servers_rules() {
        // a set refine never lowers
        assert_eq!(
            rule(|r| r.result_refine = 7).result(9),
            RefineResult::Exactly(9)
        );
        // a range with a minimum may come out lower
        let both = rule(|r| {
            r.result_min = 4;
            r.result_max = 9;
        });
        assert_eq!(both.result(11), RefineResult::Between(4, 9));
        assert_eq!(both.outcome(0), "+0 → +4 ~ +9");
        // a maximum alone only goes up from where it is
        let up = rule(|r| r.result_max = 9);
        assert_eq!(up.result(6), RefineResult::Between(6, 9));
        assert_eq!(up.result(9), RefineResult::Exactly(9));
        // a minimum alone runs to MAX_REFINE
        assert_eq!(
            rule(|r| r.result_min = 5).result(0),
            RefineResult::Between(5, MAX_REFINE)
        );
        // options only
        let options = rule(|r| r.random_options = true);
        assert_eq!(options.result(3), RefineResult::Same);
        assert_eq!(options.outcome(3), "new random options");
        assert_eq!(rule(|_| {}).outcome(3), "No change");
    }

    #[test]
    fn candidates_are_the_bags_items_it_takes_the_upgradable_first() {
        let data = DataTable::new();
        let mut character = Character::new();
        let mut worn = weapon(5);
        worn.index = 2;
        worn.wear_state = 2;
        character.inventory.add_item(worn);
        let mut high = weapon(13);
        high.index = 3;
        character.inventory.add_item(high);
        character.inventory.add_item(weapon(9));
        let mut potion = Item::from_tables(501, ItemType::Healing, &data);
        potion.index = 5;
        character.inventory.add_item(potion);
        let mut package = Item::from_tables(BOOSTER_WEAPON, ItemType::DelayConsume, &data);
        package.index = 6;
        character.inventory.add_item(package);

        let window = UpgradePackage::new(BOOSTER_WEAPON, &data).unwrap();
        let found = window.candidates(&character);
        assert_eq!(
            found,
            vec![
                Candidate {
                    index: 4,
                    fit: Ok(())
                },
                Candidate {
                    index: 2,
                    fit: Err(Unfit::Equipped)
                },
                Candidate {
                    index: 3,
                    fit: Err(Unfit::RefineTooHigh(12))
                },
            ]
        );
        assert_eq!(window.fits(&character, 4), Ok(()));
        assert_eq!(window.fits(&character, 9), Err(Unfit::NotTaken));
        assert!(window.still_held(&character));
        assert_eq!(UpgradePackage::new(501, &data), None);
    }
}
