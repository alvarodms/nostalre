//! Upgrade packages (Booster Weapon Upgrade Package...): the window the server opens when one is used (0x0ab4), the
//! item sent to it (0x0ab6), its answer (0x0ab7), and the close (0x0ab5) that lets the player act again.
//!
//! The server holds its side open from 0x0ab4 until 0x0ab5 or a warp (`pc_setpos`), and meanwhile refuses what
//! `pc_cant_act` guards (NPCs, trades, most windows). So the close is sent from one place, once per frame
//! ([`App::sync_upgrade_window`]): whenever the server's window is open and neither the desktop window nor the
//! menu's chooser shows it, however that came about (Cancel, Escape, the menu closed, the package gone, success).
use crate::App;
use nostalre_game::display_name::format_equipment_display_name;
use nostalre_game::upgrade_package::UpgradePackage;
use nostalre_ui_component::game::chat_window::ChatChannel;

const UPGRADED_COLOR: [f32; 4] = [0.6, 1.0, 0.6, 1.0];
const FAILED_COLOR: [f32; 4] = [1.0, 0.4, 0.4, 1.0];
const FAILED: &str = "The upgrade was turned down; nothing was used.";

impl App {
    /// The server opened the upgrade window for `package_id` (0x0ab4)
    pub(super) fn handle_upgrade_window_opened(&mut self, package_id: u32) {
        self.game.upgrade_open = Some(package_id);
        self.game.upgrade_sent = None;
        let Some(package) = UpgradePackage::new(package_id, &self.game.data_table) else {
            // a package the client's table lacks (the server's data newer than ours): close it at once, the
            // player stays free to act
            self.windows.chat_window.add_message(
                format!("Item {package_id} upgrades nothing this client knows of."),
                FAILED_COLOR,
                ChatChannel::System,
            );
            return;
        };
        let icons = package
            .candidates(&self.game.character)
            .iter()
            .filter_map(|c| self.game.character.inventory.get_item(c.index))
            .filter_map(|i| i.icon_path())
            .collect();
        self.preload_item_icons(icons);
        if self.windows.hud.is_modern() {
            self.windows.menu.choose_upgrade(package);
        } else {
            self.windows.upgrade_window.open(package);
        }
    }

    /// An item picked in the desktop window: asked again before the package is spent
    pub(super) fn ask_upgrade_item(&mut self, index: u16, package_id: u32, name: String) {
        let package = self.game.data_table.item_name_for(package_id, true);
        let message = format!("Upgrade {name} with the {package}? The package is used up.");
        self.game
            .arm_confirm(&mut self.windows, &message, move |yes| {
                yes.then_some(nostalre_game::event::GameEvent::RequestUpgradeItem {
                    index,
                    package_id,
                })
            });
    }

    /// Send the item to be upgraded (0x0ab6), for the window the server has open
    pub(super) fn request_upgrade_item(&mut self, index: u16, package_id: u32) {
        if self.game.upgrade_open != Some(package_id) || self.game.upgrade_sent.is_some() {
            return;
        }
        self.game.upgrade_sent = Some(index);
        self.windows.upgrade_window.sent();
        self.channel
            .send_packet(nostalre_network::build_upgrade_item_packet(
                package_id,
                index,
                self.active_packetver,
            ));
    }

    /// The server's answer (0x0ab7). On success the item came back upgraded just before it, and the window goes;
    /// on failure nothing was spent and the window stays to try again
    pub(super) fn handle_upgrade_answered(&mut self, failed: bool) {
        let sent = self.game.upgrade_sent.take();
        if failed {
            self.windows.upgrade_window.failed(FAILED.into());
            self.windows.menu.upgrade_answered(Some(FAILED.into()));
            self.windows
                .chat_window
                .add_message(FAILED.into(), FAILED_COLOR, ChatChannel::System);
            return;
        }
        self.windows.upgrade_window.close();
        self.windows.menu.upgrade_answered(None);
        let data = &self.game.data_table;
        let character = &self.game.character;
        if let Some(item) = sent.and_then(|i| character.inventory.get_item(i)) {
            let name = format_equipment_display_name(
                item,
                data.item_slot_count.as_ref(),
                data.card_name.as_ref(),
                &character.char_names,
            );
            self.windows.chat_window.add_message(
                format!("Upgraded: {name}"),
                UPGRADED_COLOR,
                ChatChannel::System,
            );
        }
    }

    /// The server shows the upgraded item (0x0b43): its description opens, as kRO's does, on the desktop HUD; the
    /// frameless one's menu is back on the bag with the item in it
    pub(super) fn handle_item_previewed(&mut self, index: u16) {
        if !self.windows.hud.is_modern() {
            self.pending_events
                .push(nostalre_game::event::GameEvent::ShowItemInfo { index });
        }
    }

    /// Once a frame: the server's window open with nothing showing it is closed on the server too (0x0ab5)
    pub(crate) fn sync_upgrade_window(&mut self) {
        if self.game.upgrade_open.is_none()
            || self.windows.upgrade_window.is_open()
            || self.windows.menu.upgrading()
        {
            return;
        }
        self.game.upgrade_open = None;
        self.game.upgrade_sent = None;
        self.channel
            .send_packet(nostalre_network::build_upgrade_close_packet(
                self.active_packetver,
            ));
    }

    /// A warp or a logout: the server has forgotten its window already, so ours goes without a close
    pub(super) fn forget_upgrade_window(&mut self) {
        self.game.upgrade_open = None;
        self.game.upgrade_sent = None;
        self.windows.upgrade_window.close();
        self.windows.menu.upgrade_answered(None);
    }
}
