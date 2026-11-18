//! Sprint and Prone: the shared state of NVF Features › Advanced Movement's two stance
//! switches [design: the owner 2026-11-18]. Both are off by default
//! (`crate::inertia::MovementInertia::sprint`, `prone`) and act only under the Global Toggle
//! ([`MovementInertia::sprint_on`], [`MovementInertia::prone_on`]). This module holds the
//! plumbing and the contract only; the features themselves (speed, AP, poses, the camera)
//! are built on it.
//!
//! # The contract
//!
//! **Switches**: read [`MovementInertia::sprint_on`] and [`MovementInertia::prone_on`]
//! (`advanced && sprint`, `advanced && prone`), never the bare fields, so the Global Toggle
//! turns both off. Command line `--sprint` / `--no-sprint`, `--prone` / `--no-prone`; rows
//! `sprint` and `prone` (registered in `crate::player`).
//!
//! **Input** (`crate::control::read_controls`, `PlaySet::Input`; struck by the scripts'
//! locks in `crate::control::enforce_locks`, the frame's input already applied):
//! - [`PlayerControl::sprint_held`]: the Sprint control (Left Shift by default) is held and
//!   Sprint is on. Struck by the movement lock.
//! - [`PlayerControl::prone_hold`]: seconds the Prone control has been held without a break
//!   while Prone is on (0 when not held).
//! - [`PlayerControl::prone_request`]: an edge, set once per press: on Crouch/Sneak's own
//!   key (the default, [`Bindings::prone_shares_crouch`]) when the hold reaches
//!   [`PRONE_HOLD_SECONDS`]; on a key of the player's own the moment it is pressed (a
//!   toggle, no hold). The Prone feature clears it once it has acted (as `crate::combat`
//!   clears `PlayerControl::reload`). Struck with the hold by the sneaking lock.
//! - On the shared key Crouch/Sneak itself still toggles sneaking on the press
//!   (`PlayerControl::sneak`, before the hold is known): the Prone feature decides what a
//!   completed hold means for it (prone needs `sneak` true, below).
//!
//! **Keys** (`crate::bindings`): Sprint and Prone are bindings of their own (Controls ›
//! Movement), greyed and not rebindable while their switch is off ([`binding_greyed`]); off,
//! they act on nothing. While Sprint is on, Left Shift is Sprint's and Run acts on X, unless
//! the player moved Run off its default ([`Bindings::effective`], worked out each time, never
//! saved). Prone shares Crouch/Sneak's key on purpose; a rebind never swaps within those pairs
//! ([`crate::bindings::SHARED_PAIRS`]). [`sync_bindings`] keeps [`Bindings::sprint_on`] and
//! [`Bindings::prone_on`] on the switches.
//!
//! **Motion** ([`StanceMotion`], one resource both features and the camera and animation
//! code read): the Sprint feature writes [`StanceMotion::sprinting`],
//! [`StanceMotion::sprint_share`] and [`StanceMotion::winded`]; the Prone feature writes
//! [`StanceMotion::prone`] and [`StanceMotion::prone_share`]. Nothing else writes them;
//! everything may read them. With a switch off its feature leaves its fields at rest (false,
//! 0) after easing out.
//!
//! **Prone is a sub-state of sneaking** [design: the owner 2026-11-18: "prone must give the
//! same sneak bonuses as crouching (it counts as sneaking)"]: while [`StanceMotion::prone`]
//! is true the Prone feature keeps `crate::control::PlayerControl::sneak` true (and so
//! `crate::player::MoveIntent::sneak`), so every reader of the sneak state sees the player as
//! sneaking without a change of its own:
//! - the scripts and perks: `crate::control::tell_scripts` writes `PlayerControl::sneak` into
//!   the runtime's motion (`fnv_game` `Game::set_motion`, `sneaking`), which `IsSneaking`
//!   (`crates/fnv-game/src/functions/senses.rs`, `Game::is_sneaking`) and so every perk entry
//!   point and condition on it read (`crates/fnv-game/src/runner.rs`), and the HUD bar's
//!   visibility (`crates/fnv-game/src/ui_hudbar.rs`), pickpocketing (`crate::activate`), the
//!   companion wheel and the compass;
//! - detection: nvforever `detection.rs` (the player's `sneaking` from `PlayerControl::sneak`)
//!   and `crates/fnv-game/src/detection.rs` (the Sneak skill against the senses);
//! - combat: sneak attack criticals (`crate::combat`'s `sneak_attack` on
//!   `PlayerControl::sneak`; `crates/fnv-combat/src/damage.rs`) and the spread's standing
//!   penalty (`crate::combat`'s `sneaking: control.sneak`, `crates/fnv-combat/src/spread.rs`);
//! - the HUD's sneak label (`crate::hud`) and the sneak speed (`crate::player`'s
//!   `MoveIntent::sneak`, `fMoveSneakMult`).
//!
//! The scripts' sneaking lock (`crate::control::ControlLocks::sneaking`) strikes
//! `PlayerControl::sneak` and so stands a prone player up too.
//!
//! **Sprint and the perks**: the speed perks and stats already scale every gait
//! (`crate::player::PerkSpeed`, Modify Run Speed perks; `crate::player::StatSpeed`, Speed
//! Mult and armour; `crate::player::RunPace`): the Sprint feature multiplies its sprint speed
//! on top of the run speed they give, so they apply to it, and spends AP through the same
//! paths the game's AP costs use (`crate::combat`, the AP actor value), so the AP perks apply.
//!
//! [`MovementInertia::sprint_on`]: crate::inertia::MovementInertia::sprint_on
//! [`MovementInertia::prone_on`]: crate::inertia::MovementInertia::prone_on
//! [`PlayerControl::sprint_held`]: crate::control::PlayerControl::sprint_held
//! [`PlayerControl::prone_hold`]: crate::control::PlayerControl::prone_hold
//! [`PlayerControl::prone_request`]: crate::control::PlayerControl::prone_request
//! [`Bindings::prone_shares_crouch`]: crate::bindings::Bindings::prone_shares_crouch
//! [`Bindings::effective`]: crate::bindings::Bindings::effective
//! [`Bindings::sprint_on`]: crate::bindings::Bindings::sprint_on
//! [`Bindings::prone_on`]: crate::bindings::Bindings::prone_on

use crate::bindings::{Bindings, Control};
use crate::inertia::MovementInertia;
use bevy::prelude::*;

/// On Crouch/Sneak's key, holding it this long asks to lie down
/// (`crate::control::PlayerControl::prone_request`) [design: the owner 2026-11-18, "hold
/// Crouch/Sneak for a second"].
pub const PRONE_HOLD_SECONDS: f32 = 1.0;

/// What the Sprint and Prone features are doing now, for each other, the camera and the
/// animation code (module notes: who writes which field).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct StanceMotion {
    /// The share of a sprint now, 0 (none) to 1 (a full sprint), eased in and out by the
    /// Sprint feature: what the camera, the arms and the body blend by. Written by Sprint.
    pub sprint_share: f32,
    /// The share of lying down now, 0 (standing or crouched) to 1 (flat on the ground), eased
    /// by the Prone feature: what the eye height, the collider, the camera and the body blend
    /// by. Written by Prone.
    pub prone_share: f32,
    /// Sprinting is the target state (the key held, moving forward, AP left). Written by
    /// Sprint.
    pub sprinting: bool,
    /// Lying down is the target state (asked for and not stood up again). While true the
    /// player also sneaks (`crate::control::PlayerControl::sneak`; module notes). Written by
    /// Prone.
    pub prone: bool,
    /// Out of breath after a sprint, 0 (not) to 1 (fully winded): the strength of the view's
    /// and the gun's breathing shake. Written by Sprint.
    pub winded: f32,
}

/// Why control `c`'s Controls row is greyed now (Sprint or Prone with its switch off, or the
/// Global Toggle off), in the words the page shows under the row; `None`: it is not. A greyed
/// binding cannot be rebound or cleared (`crate::bindings::rebind`). Without
/// [`MovementInertia`] nothing is greyed.
pub fn binding_greyed(world: &World, c: Control) -> Option<&'static str> {
    let m = world.get_resource::<MovementInertia>()?;
    let (own, why) = match c {
        Control::Sprint => (m.sprint, SPRINT_OFF),
        Control::Prone => (m.prone, PRONE_OFF),
        _ => return None,
    };
    if !own {
        Some(why)
    } else if !m.advanced {
        Some(GLOBAL_TOGGLE_OFF)
    } else {
        None
    }
}

/// What a greyed Sprint or Prone binding's note says ([`binding_greyed`]).
pub const SPRINT_OFF: &str = "Turn on Sprint in NVF Features > Advanced Movement first.";
pub const PRONE_OFF: &str = "Turn on Prone in NVF Features > Advanced Movement first.";
pub const GLOBAL_TOGGLE_OFF: &str = "Turn on the Global Toggle in NVF Features > Advanced Movement first.";

/// Keep [`Bindings::sprint_on`] and [`Bindings::prone_on`] on the switches, so the controls
/// act on the effective keys (`crate::bindings::Bindings::effective`) from the next read.
pub fn sync_bindings(inertia: Option<Res<MovementInertia>>, bindings: Option<ResMut<Bindings>>) {
    let (Some(m), Some(mut b)) = (inertia, bindings) else { return };
    let now = (m.sprint_on(), m.prone_on());
    if (b.sprint_on, b.prone_on) != now {
        (b.sprint_on, b.prone_on) = now;
        let run = b.effective(Control::Run);
        let moved = if run != b.get(Control::Run) { format!("; Run on {} while Sprint has {}", run.text(), b.get(Control::Run).text()) } else { String::new() };
        info!("controls: Sprint {}, Prone {}{moved}", on_off(now.0), on_off(now.1));
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// [`StanceMotion`] and the bindings kept on the switches ([`sync_bindings`], before the
/// frame's input).
pub struct StancePlugin;

impl Plugin for StancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StanceMotion>().add_systems(PreUpdate, sync_bindings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bindings_follow_the_switches_and_greyed_rows_say_why() {
        let mut app = App::new();
        app.init_resource::<Bindings>().init_resource::<MovementInertia>().add_plugins(StancePlugin);
        app.update();
        assert_eq!(*app.world().resource::<StanceMotion>(), StanceMotion::default());
        let b = app.world().resource::<Bindings>();
        assert!(!b.sprint_on && !b.prone_on, "both off by default");
        assert_eq!(binding_greyed(app.world(), Control::Sprint), Some(SPRINT_OFF));
        assert_eq!(binding_greyed(app.world(), Control::Prone), Some(PRONE_OFF));
        assert_eq!(binding_greyed(app.world(), Control::Run), None);
        {
            let mut m = app.world_mut().resource_mut::<MovementInertia>();
            (m.sprint, m.prone) = (true, true);
        }
        app.update();
        let b = app.world().resource::<Bindings>();
        assert!(b.sprint_on && b.prone_on);
        assert_eq!(b.effective(Control::Run).key, Some(crate::bindings::RUN_SPARE_KEY));
        assert_eq!(binding_greyed(app.world(), Control::Sprint), None);
        // The Global Toggle off: both act on nothing, greyed for it, each keeps its choice.
        app.world_mut().resource_mut::<MovementInertia>().advanced = false;
        app.update();
        let b = app.world().resource::<Bindings>();
        assert!(!b.sprint_on && !b.prone_on);
        assert_eq!(b.effective(Control::Run).key, Some(KeyCode::ShiftLeft), "Shift is Run's again");
        assert_eq!(binding_greyed(app.world(), Control::Prone), Some(GLOBAL_TOGGLE_OFF));
        // A greyed binding is not rebound.
        app.world_mut().resource_mut::<MovementInertia>().advanced = true;
        app.world_mut().resource_mut::<MovementInertia>().prone = false;
        crate::bindings::rebind(app.world_mut(), Control::Prone, crate::bindings::Input::Key(KeyCode::KeyK));
        assert_eq!(app.world().resource::<Bindings>().get(Control::Prone).key, Some(KeyCode::ControlLeft));
        crate::bindings::clear_key(app.world_mut(), Control::Prone);
        assert_eq!(app.world().resource::<Bindings>().get(Control::Prone).key, Some(KeyCode::ControlLeft));
        crate::bindings::rebind(app.world_mut(), Control::Sprint, crate::bindings::Input::Key(KeyCode::KeyK));
        assert_eq!(app.world().resource::<Bindings>().get(Control::Sprint).key, Some(KeyCode::KeyK), "Sprint is on: it rebinds");
    }
}
