//! Selection boxes (kRO's item package selection, from 20220216): an item, such as the Metal Weapon Box, that opens a
//! window of choices instead of being used. The player's pick goes to the server as the choice's number
//! (CZ_USE_PACKAGEITEM, 0x0baf); the server takes one box and gives what its `item_packages.yml` lists under that
//! number, as plain inventory updates, with no answer of its own.
//!
//! kRO's client draws the window from its own table (`selectpackageitem_cln.lub`). The choices here come from the
//! server's data instead (`package_tables`, generated), so what is shown is what is given; the choices' names are
//! kRO's table's when it has the box ([`DataTable::package_names`]), else made from what they hold.

use crate::data_table::DataTable;
use crate::item::{Item, ItemType};
use crate::package_tables::package_choices;
use models::enums::EnumWithNumberValue;

/// One thing a choice gives, as the server's item_packages.yml lists it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageItem {
    pub item_id: u32,
    /// The item's kind as the server sends it in the inventory (rAthena's `IT_*`, clif's `itemtype`)
    pub item_type: u8,
    pub amount: u16,
    /// Hours until it expires; 0 for an item that does not
    pub rental_hours: u32,
    pub refine: u8,
    /// The enchant grade (0 none, 1 D to 4 A)
    pub grade: u8,
    /// Whether it comes with random options (rolled by the server)
    pub random_option: bool,
}

/// One choice of a box: the number the server knows it by (its `Group`), and what it gives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageChoice {
    pub group: u32,
    pub items: &'static [PackageItem],
}

/// A choice ready to show: its name, and what it gives
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub group: u32,
    pub name: String,
    pub items: &'static [PackageItem],
}

/// A selection box in the bag, opened: what it is, where it is, and its choices
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectPackage {
    /// The box's inventory index, as the client numbers it
    pub index: u16,
    pub box_id: u32,
    /// The box's name, the window's title
    pub title: String,
    pub choices: Vec<Choice>,
}

/// Whether the item is a selection box the server knows
pub fn is_select_package(item_id: u32) -> bool {
    package_choices(item_id).is_some_and(|c| !c.is_empty())
}

impl SelectPackage {
    /// The box at `index` (`box_id`) opened, `None` when it is not a selection box
    pub fn new(index: u16, box_id: u32, data: &DataTable) -> Option<Self> {
        let choices = package_choices(box_id).filter(|c| !c.is_empty())?;
        let names = data.package_names.get(&box_id);
        Some(Self {
            index,
            box_id,
            title: data.item_name_for(box_id, true),
            choices: choices
                .iter()
                .map(|c| Choice {
                    group: c.group,
                    name: names
                        .and_then(|n| n.get(&c.group))
                        .filter(|n| !n.trim().is_empty())
                        .cloned()
                        .unwrap_or_else(|| choice_name(c.items, data)),
                    items: c.items,
                })
                .collect(),
        })
    }
}

impl PackageItem {
    /// As a bag item, for the item panels: its name, look, refine and grade
    pub fn to_item(&self, data: &DataTable) -> Item {
        let mut item = Item::from_tables(
            self.item_id,
            ItemType::from_value(usize::from(self.item_type)),
            data,
        );
        item.refining_level = self.refine;
        item.grade = self.grade;
        item.count = i16::try_from(self.amount).unwrap_or(i16::MAX);
        item
    }

    /// Its name as a choice shows it: "+7 Metal Lance", "White Potion ×10"
    pub fn label(&self, data: &DataTable) -> String {
        let name = data.item_name_for(self.item_id, true);
        let mut label = if self.refine > 0 {
            format!("+{} {name}", self.refine)
        } else {
            name
        };
        if self.amount > 1 {
            label.push_str(&format!(" ×{}", self.amount));
        }
        label
    }
}

/// A choice's name when kRO's table has none: what it gives, one after the other
fn choice_name(items: &[PackageItem], data: &DataTable) -> String {
    items
        .iter()
        .map(|i| i.label(data))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const METAL_WEAPON_BOX: u32 = 101454;

    #[test]
    fn the_metal_weapon_box_has_its_eleven_weapons_at_plus_seven() {
        let choices = package_choices(METAL_WEAPON_BOX).expect("in item_packages.yml");
        assert_eq!(choices.len(), 11);
        assert_eq!(
            choices.iter().map(|c| c.group).collect::<Vec<_>>(),
            (0..11).collect::<Vec<_>>()
        );
        assert!(
            choices
                .iter()
                .all(|c| c.items.len() == 1 && c.items[0].refine == 7)
        );
        // the first is the Metal Two-Handed Sword, a weapon
        assert_eq!(choices[0].items[0].item_id, 21005);
        assert_eq!(choices[0].items[0].item_type, 5);
        assert!(is_select_package(METAL_WEAPON_BOX));
        assert!(!is_select_package(501));
    }

    #[test]
    fn a_choice_takes_kros_name_and_makes_one_without_it() {
        let mut data = DataTable::new();
        data.package_names.insert(
            METAL_WEAPON_BOX,
            HashMap::from([(1, "+7 Metal Lance".to_string()), (2, " ".to_string())]),
        );
        let package = SelectPackage::new(9, METAL_WEAPON_BOX, &data).unwrap();
        assert_eq!(package.index, 9);
        assert_eq!(package.choices[1].name, "+7 Metal Lance");
        // a blank name or none at all: made from the item ("Item #id" without the item tables)
        assert_eq!(package.choices[2].name, "+7 Item #16023");
        assert_eq!(package.choices[0].name, "+7 Item #21005");
        assert_eq!(SelectPackage::new(9, 501, &data), None);
    }

    #[test]
    fn a_label_shows_refine_and_amount() {
        let data = DataTable::new();
        let item = |refine, amount| PackageItem {
            item_id: 504,
            item_type: 0,
            amount,
            rental_hours: 0,
            refine,
            grade: 0,
            random_option: false,
        };
        assert_eq!(item(0, 10).label(&data), "Item #504 ×10");
        assert_eq!(item(4, 1).label(&data), "+4 Item #504");
        let bag = item(4, 3).to_item(&data);
        assert_eq!((bag.refining_level, bag.count), (4, 3));
    }
}
