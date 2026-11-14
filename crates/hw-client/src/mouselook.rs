//! Always-on mouselook: the pointer is hidden and pinned, and the mouse
//! turns the camera without a button held (the style of most action games).
//! The other style, hold the right button to look, stays the default of the
//! engine's input and is what the setting off means.
//!
//! The pointer is freed on its own whenever the game needs a cursor: any
//! window or menu, the chat line, the map, the police computer, a taxi or
//! tube list, and while Alt is held. When the need ends it is pinned again.
//! Escape and losing focus drop the pin from outside (Engine does that); the
//! look then waits for a click in the world, so a window that was not in
//! front never takes the pointer by itself.
//!
//! This is only the decision, one call a frame, so the rules can be tested
//! without a window ([`Mouselook::update`]).

/// What the mouselook knows from frame to frame. The default pins nothing:
/// the pointer is not taken before the player has clicked the game.
#[derive(Debug, Default)]
pub struct Mouselook {
    /// We pinned the pointer last frame.
    pinned: bool,
    /// The pointer was freed because something needed a cursor: it is
    /// pinned again by itself when that ends.
    freed_for_ui: bool,
}

impl Mouselook {
    /// Call once a frame after every window has been drawn.
    ///
    /// - `always`: the setting is on (off: this does nothing and forgets).
    /// - `cursor_needed`: a window, menu or Alt wants a cursor now.
    /// - `locked`: the pointer is pinned right now (Engine clears it on
    ///   Escape and on losing focus).
    /// - `clicked_in_world`: the left button went down over the game, not
    ///   over a window.
    ///
    /// Returns `Some(lock)` when the pointer's pin should change.
    pub fn update(
        &mut self,
        always: bool,
        cursor_needed: bool,
        locked: bool,
        clicked_in_world: bool,
    ) -> Option<bool> {
        if !always {
            *self = Self::default();
            return None;
        }
        if cursor_needed {
            self.pinned = false;
            self.freed_for_ui = true;
            return locked.then_some(false);
        }
        if locked {
            self.pinned = true;
            self.freed_for_ui = false;
            return None;
        }
        // Not pinned, and nothing needs the cursor.
        if self.freed_for_ui {
            // The window or Alt that took it is gone.
            self.freed_for_ui = false;
            self.pinned = true;
            return Some(true);
        }
        if self.pinned {
            // Dropped from outside (Escape, focus lost): wait for a click.
            self.pinned = false;
        }
        if clicked_in_world {
            self.pinned = true;
            return Some(true);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_a_click_then_pins() {
        let mut m = Mouselook::default();
        assert_eq!(m.update(true, false, false, false), None);
        assert_eq!(m.update(true, false, false, true), Some(true));
        assert_eq!(m.update(true, false, true, false), None);
    }

    #[test]
    fn a_window_frees_it_and_closing_pins_it_again() {
        let mut m = Mouselook::default();
        m.update(true, false, false, true);
        m.update(true, false, true, false);
        assert_eq!(m.update(true, true, true, false), Some(false));
        assert_eq!(m.update(true, true, false, false), None);
        assert_eq!(m.update(true, false, false, false), Some(true));
    }

    #[test]
    fn escape_or_lost_focus_waits_for_a_click() {
        let mut m = Mouselook::default();
        m.update(true, false, false, true);
        m.update(true, false, true, false);
        // Engine dropped the pin and nothing needs a cursor.
        assert_eq!(m.update(true, false, false, false), None);
        assert_eq!(m.update(true, false, false, false), None);
        assert_eq!(m.update(true, false, false, true), Some(true));
    }

    #[test]
    fn menu_opened_by_escape_gives_the_pointer_back_when_closed() {
        let mut m = Mouselook::default();
        m.update(true, false, false, true);
        m.update(true, false, true, false);
        // Escape: the menu opens the same frame the pin is still set.
        assert_eq!(m.update(true, true, true, false), Some(false));
        assert_eq!(m.update(true, true, false, false), None);
        assert_eq!(m.update(true, false, false, false), Some(true));
    }

    #[test]
    fn setting_off_never_touches_the_pointer() {
        let mut m = Mouselook::default();
        assert_eq!(m.update(false, false, false, true), None);
        assert_eq!(m.update(false, true, true, false), None);
    }
}
