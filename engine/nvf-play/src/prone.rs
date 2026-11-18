//! Prone: NVF Features › Advanced Movement › Prone [design: the owner 2026-11-18], on the
//! shared contract of [`crate::stance`] (read it first: prone is a sub-state of sneaking).
//!
//! **Lying down.** With [`MovementInertia::prone_on`], a held Crouch/Sneak (a second, on its
//! own key; [`crate::stance::PRONE_HOLD_SECONDS`]) or a press of Prone's own key
//! (`PlayerControl::prone_request`) lies the player down, standing still on the ground (not
//! swimming, seated, held by an idle, or with a movement key held). On the shared key the
//! press has already crouched the player, so they go on smoothly from the crouch down.
//! While lying [`StanceMotion::prone`] is true and `PlayerControl::sneak` is kept true: prone
//! counts as sneaking for every reader of it (the list in `crate::stance`).
//!
//! **Getting up** [design, the owner: "any movement key makes the character stand up"]:
//! - a movement control (Forward, Back, a strafe, Auto Move, Jump): up to standing; the
//!   player stops sneaking unless they are otherwise sneaking (Hold to Sneak with the key
//!   still held). The movement is held back until the body is mostly up
//!   ([`MOVE_FROM_SHARE`]) and the jump is taken as the getting up;
//! - Crouch/Sneak pressed (or Prone's own key pressed, or the hold asked again): up to the
//!   crouch, still sneaking [design: this module's choice, "pressing it again while prone →
//!   back to crouch"]; the hold of that press does not lie the player down again;
//! - the scripts' sneaking lock or a script ending the player's sneaking: up to standing;
//! - the ground gone (in the air [`AIR_SECONDS`], swimming), furniture or an idle taking the
//!   player, or Prone switched off: up to the crouch.
//!
//! **The share** ([`StanceMotion::prone_share`]): 0 to 1 on a critically damped spring,
//! [`DOWN_SECONDS`] down and [`UP_SECONDS`] up (95 %), the rate chosen when it leaves rest
//! and kept through a change of mind (a spring's constants never switch mid-motion), so it
//! starts and ends with no speed and turns back without a jump. The camera, the body and
//! the bonuses blend by it.
//!
//! **Bonuses** (full only flat on the ground, none with the share at 0, so with Prone off
//! everything is as the original): [`spread_mult`] (the shot's and the crosshair's cone,
//! iron sights, scopes and the hip alike: `crate::combat`), [`recoil_mult`] (the view's kick,
//! `crate::combat`; the gun's on nvforever's arms), [`sway_mult`] (a scope's sway, nvforever
//! `scope_view.rs`) and [`vats_bonus`] (V.A.T.S. chances, added before the game's cap:
//! `crate::vats_mode`).
//!
//! The view: [`pitch_floor`] keeps the view from looking straight down into the ground
//! while lying; nvforever's `prone_view.rs` lowers the first-person view and lays the
//! third-person body down.
//!
//! Log lines: `prone: lying down (…)`, `prone: flat on the ground …`, `prone: getting up
//! (…)`, `prone: up …`, `prone summary: …` at exit.
//!
//! [`MovementInertia::prone_on`]: crate::inertia::MovementInertia::prone_on

use crate::bindings::{Control, ControlInput};
use crate::control::{ControlLocks, PlayerControl};
use crate::ease::{omega_for, Spring};
use crate::inertia::MovementInertia;
use crate::player::{MoveIntent, Player, MAX_PITCH};
use crate::stance::StanceMotion;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;

/// Seconds the share takes from 0 to 95 % lying down (from the crouch) [design: the owner's
/// brief, "down from crouch ~1.2 s"].
pub const DOWN_SECONDS: f32 = 1.2;
/// Seconds the share takes from 1 to 5 % getting up [design: the owner's brief, "stand up
/// ~1.3 s"].
pub const UP_SECONDS: f32 = 1.3;
/// Getting up, the movement keys move the player again once the share is under this: the
/// body is mostly up [design].
pub const MOVE_FROM_SHARE: f32 = 0.35;
/// In the air this long, a lying player gets up (the ground went from under them) [design].
pub const AIR_SECONDS: f32 = 0.25;

/// The cone of a shot (and the crosshair's) flat on the ground: this share of it [design:
/// the owner's brief, "spread ×0.5"; tunable].
pub const SPREAD: f32 = 0.5;
/// The view's kick of a shot (and the gun's) flat on the ground [design: "recoil ×0.5"].
pub const RECOIL: f32 = 0.5;
/// A scope's sway flat on the ground [design: "scope sway ×0.5"].
pub const SWAY: f32 = 0.5;
/// V.A.T.S. chances flat on the ground: this many percentage points more, before the game's
/// cap (`fnv_combat::vats::final_chance`, 95 %) [design: "VATS +15 percentage points"].
pub const VATS_POINTS: f32 = 15.0;
/// Lying down the view looks at most this far down, degrees [design: lower, and the view
/// looks into the ground from 17 units up].
pub const PITCH_DOWN_DEG: f32 = 40.0;

/// `1` to `full` by the share (0 to 1).
fn by_share(share: f32, full: f32) -> f32 {
    1.0 + (full - 1.0) * share.clamp(0.0, 1.0)
}

/// The share of a shot's cone lying down by `share` ([`SPREAD`] flat; 1 standing).
pub fn spread_mult(share: f32) -> f32 {
    by_share(share, SPREAD)
}

/// The share of a shot's kick lying down by `share` ([`RECOIL`] flat).
pub fn recoil_mult(share: f32) -> f32 {
    by_share(share, RECOIL)
}

/// The share of a scope's sway lying down by `share` ([`SWAY`] flat).
pub fn sway_mult(share: f32) -> f32 {
    by_share(share, SWAY)
}

/// The percentage points a V.A.T.S. chance gains lying down by `share` ([`VATS_POINTS`]
/// flat); the chance is capped after it as before.
pub fn vats_bonus(share: f32) -> f32 {
    VATS_POINTS * share.clamp(0.0, 1.0)
}

/// The view's lowest pitch (radians, negative) lying down by `share`: straight down's limit
/// standing, [`PITCH_DOWN_DEG`] flat.
pub fn pitch_floor(share: f32) -> f32 {
    let s = share.clamp(0.0, 1.0);
    -(MAX_PITCH + (PITCH_DOWN_DEG.to_radians() - MAX_PITCH) * s)
}

/// What the frame brings the Prone feature (read from the controls and the world).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Inputs {
    /// Prone is on (`MovementInertia::prone_on`).
    pub on: bool,
    /// Seconds the Prone control is held (`PlayerControl::prone_hold`).
    pub hold: f32,
    /// Crouch/Sneak pressed this frame, and let go.
    pub crouch_pressed: bool,
    pub crouch_released: bool,
    /// Crouch/Sneak held now, and the Hold to Sneak tweak on.
    pub crouch_held: bool,
    pub hold_to_sneak: bool,
    /// A movement control asks to move (the keys' intent, Auto Move, Jump).
    pub moving: bool,
    /// The scripts' sneaking lock is on.
    pub locked: bool,
    /// Standing on the ground (not in the air, not swimming).
    pub grounded: bool,
    pub swimming: bool,
    /// Furniture holds the player, or an idle drives the view.
    pub held: bool,
}

/// What changed this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Lying down.
    Down,
    /// Up to the crouch (still sneaking), and why.
    ToCrouch(&'static str),
    /// Up to standing, and why.
    Stand(&'static str),
}

/// The Prone feature's state.
#[derive(Resource, Debug, Clone, Default)]
pub struct Prone {
    /// Lying down is the target ([`StanceMotion::prone`]).
    pub prone: bool,
    /// The share on its spring ([`StanceMotion::prone_share`]).
    pub share: Spring,
    /// The spring's rate, chosen when the share left rest.
    omega: f32,
    /// The hold that left the prone pose: it does not ask to lie down again.
    ignore_hold: bool,
    /// Seconds in the air while lying.
    air: f32,
    /// Reported flat on the ground (a log line once each time).
    told_flat: bool,
    /// For the exit summary: times down, up by moving, to the crouch, other.
    pub downs: u32,
    pub stood: u32,
    pub crouched: u32,
    /// Requests that could not lie down (moving, in the air, …).
    pub refused: u32,
}

impl Prone {
    /// The share now (0 standing or crouched, 1 flat).
    pub fn value(&self) -> f32 {
        self.share.value
    }

    /// One frame: the controls' `sneak` and `request` (`PlayerControl::sneak`,
    /// `prone_request`) are read and written; the share moves `dt` seconds. Returns what
    /// changed.
    pub fn frame(&mut self, i: &Inputs, sneak: &mut bool, request: &mut bool, dt: f32) -> Option<Change> {
        let change = self.decide(i, sneak, request, dt);
        match change {
            Some(Change::Down) => self.downs += 1,
            Some(Change::Stand(_)) => self.stood += 1,
            Some(Change::ToCrouch(_)) => self.crouched += 1,
            None => {}
        }
        if self.prone {
            // Lying is sneaking (`crate::stance`).
            *sneak = true;
        }
        self.ease(dt);
        change
    }

    fn decide(&mut self, i: &Inputs, sneak: &mut bool, request: &mut bool, dt: f32) -> Option<Change> {
        if i.hold <= 0.0 {
            self.ignore_hold = false;
        }
        let asked = std::mem::take(request) && i.on;
        if !i.on {
            return self.prone.then(|| self.leave(Change::ToCrouch("Prone switched off"), sneak, true));
        }
        if self.prone {
            self.air = if i.grounded { 0.0 } else { self.air };
            if !*sneak {
                // Something took sneaking away this frame.
                return Some(if i.locked {
                    self.leave(Change::Stand("a script's sneaking lock"), sneak, false)
                } else if i.crouch_pressed {
                    // Crouch/Sneak pressed again: up to the crouch; its hold (on the shared
                    // key) does not lie down again.
                    self.ignore_hold = true;
                    self.leave(Change::ToCrouch("Crouch/Sneak pressed"), sneak, true)
                } else if i.hold_to_sneak && i.crouch_released {
                    // Hold to Sneak: letting go of the key that laid the player down keeps them
                    // lying.
                    *sneak = true;
                    return None;
                } else {
                    self.leave(Change::Stand("sneaking ended"), sneak, false)
                });
            }
            if asked && !self.ignore_hold {
                self.ignore_hold = true;
                return Some(self.leave(Change::ToCrouch("Prone asked again"), sneak, true));
            }
            if i.moving {
                let still = i.hold_to_sneak && i.crouch_held;
                return Some(self.leave(Change::Stand("moved"), sneak, still));
            }
            if i.swimming || i.held {
                return Some(self.leave(Change::ToCrouch(if i.swimming { "swimming" } else { "furniture or an idle took the player" }), sneak, true));
            }
            if !i.grounded {
                self.air += dt.max(0.0);
            }
            if self.air >= AIR_SECONDS {
                return Some(self.leave(Change::ToCrouch("the ground went"), sneak, true));
            }
            return None;
        }
        if !asked || self.ignore_hold {
            return None;
        }
        if i.moving || !i.grounded || i.swimming || i.held || i.locked {
            self.refused += 1;
            info!(
                "prone: not lying down ({})",
                if i.moving {
                    "moving"
                } else if i.swimming {
                    "swimming"
                } else if i.held {
                    "furniture or an idle holds the player"
                } else if i.locked {
                    "a script's sneaking lock"
                } else {
                    "in the air"
                }
            );
            return None;
        }
        self.prone = true;
        self.air = 0.0;
        *sneak = true;
        Some(Change::Down)
    }

    /// Stop lying: sneaking stays `still_sneaking`.
    fn leave(&mut self, c: Change, sneak: &mut bool, still_sneaking: bool) -> Change {
        self.prone = false;
        self.told_flat = false;
        *sneak = still_sneaking;
        c
    }

    /// `dt` seconds of the share's spring towards the target.
    fn ease(&mut self, dt: f32) {
        let target = f32::from(u8::from(self.prone));
        let at_rest = self.share.rate == 0.0 && (self.share.value == 0.0 || self.share.value == 1.0);
        if at_rest || self.omega <= 0.0 {
            // A motion starts: its rate for the whole of it, whichever way it later turns.
            self.omega = omega_for(if self.prone { DOWN_SECONDS } else { UP_SECONDS });
        }
        if self.share.value == target && self.share.rate == 0.0 {
            return;
        }
        self.share.step(target, self.omega, dt);
        // The exponential tail never arrives by itself: at rest once a hair from the end.
        self.share.snap(target, 1e-4, 1e-3);
    }
}

/// The frame's [`Inputs`] from the controls and the world.
#[allow(clippy::too_many_arguments)]
fn inputs(
    input: &ControlInput,
    inertia: Option<&MovementInertia>,
    tweaks: Option<&crate::tweaks::TweakSettings>,
    locks: &ControlLocks,
    control: &PlayerControl,
    intent: &MoveIntent,
    phys: Option<&crate::phys::PhysState>,
    held: bool,
) -> Inputs {
    let (grounded, swimming) = phys.and_then(|p| p.player.as_ref()).map_or((true, false), |c| (c.grounded, c.swimming));
    Inputs {
        on: inertia.is_some_and(MovementInertia::prone_on),
        hold: control.prone_hold,
        crouch_pressed: input.just_pressed(Control::CrouchSneak),
        crouch_released: input.just_released(Control::CrouchSneak),
        crouch_held: input.pressed(Control::CrouchSneak),
        hold_to_sneak: tweaks.is_some_and(|t| t.hold_to_sneak),
        moving: intent.forward != 0.0 || intent.right != 0.0 || intent.jump || control.auto_move,
        locked: locks.sneaking,
        grounded,
        swimming,
        held,
    }
}

/// Where the player is held: the character (on the ground, swimming), furniture, an idle
/// driving the view.
type Holders<'w> = (Option<Res<'w, crate::phys::PhysState>>, Option<Res<'w, crate::furniture::PlayerSeat>>, Option<Res<'w, crate::player::ViewDriven>>);

/// The Prone feature each frame, after the controls and the scripts' locks, before the
/// physics step moves the player and before the scripts are told whether they sneak.
#[allow(clippy::too_many_arguments)]
pub fn update(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    input: ControlInput,
    (inertia, tweaks, locks): (Option<Res<MovementInertia>>, Option<Res<crate::tweaks::TweakSettings>>, Res<ControlLocks>),
    (mut control, mut intent, mut player): (ResMut<PlayerControl>, ResMut<MoveIntent>, ResMut<Player>),
    (phys, seat, driven): Holders,
    mut prone: ResMut<Prone>,
    mut motion: ResMut<StanceMotion>,
) {
    if *state.get() != PlayState::Playing {
        return;
    }
    let held = seat.is_some_and(|s| s.hold.is_some()) || driven.is_some_and(|d| d.0);
    let i = inputs(&input, inertia.as_deref(), tweaks.as_deref(), &locks, &control, &intent, phys.as_deref(), held);
    let was = prone.value();
    let c = &mut *control;
    let change = prone.frame(&i, &mut c.sneak, &mut c.prone_request, time.delta_secs());
    match change {
        Some(Change::Down) => info!("prone: lying down ({}; share {was:.2})", if input.bindings.prone_shares_crouch() { "Crouch/Sneak held" } else { "Prone pressed" }),
        Some(Change::ToCrouch(why)) => info!("prone: getting up to the crouch ({why}; share {was:.2})"),
        Some(Change::Stand(why)) => info!("prone: getting up ({why}; share {was:.2})"),
        None => {}
    }
    let share = prone.value();
    if prone.prone && share >= 1.0 && !prone.told_flat {
        prone.told_flat = true;
        info!("prone: flat on the ground: spread x{:.2}, recoil x{:.2}, scope sway x{:.2}, V.A.T.S. +{:.0} points", spread_mult(share), recoil_mult(share), sway_mult(share), vats_bonus(share));
    }
    if !prone.prone && was > 0.0 && share == 0.0 {
        info!("prone: up");
    }
    // No crawling: the movement waits until the body is mostly up; a jump is the getting up.
    if (prone.prone || share > MOVE_FROM_SHARE) && (intent.forward != 0.0 || intent.right != 0.0 || intent.jump) {
        (intent.forward, intent.right, intent.jump) = (0.0, 0.0, false);
    }
    if intent.sneak != control.sneak {
        intent.sneak = control.sneak;
    }
    // The view does not look into the ground while lying.
    if share > 0.0 {
        let floor = pitch_floor(share);
        if player.pitch < floor {
            player.pitch = floor;
        }
    }
    let now = StanceMotion { prone: prone.prone, prone_share: share, ..*motion };
    motion.set_if_neq(now);
}

/// At exit, when the player lay down at all.
pub fn report(mut exit: MessageReader<AppExit>, prone: Res<Prone>) {
    if exit.read().next().is_none() || prone.downs + prone.refused == 0 {
        return;
    }
    info!("prone summary: lay down {} time(s), got up {} by moving or a script, {} to the crouch, {} request(s) refused; lying at exit: {}", prone.downs, prone.stood, prone.crouched, prone.refused, prone.prone);
}

/// [`Prone`] and its systems.
pub struct PronePlugin;

impl Plugin for PronePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Prone>()
            // Only where the keys are read (a test app without input has none).
            .add_systems(
                Update,
                update
                    .after(crate::control::enforce_locks)
                    .before(crate::control::tell_scripts)
                    .after(PlaySet::Input)
                    .before(PlaySet::Physics)
                    .run_if(resource_exists::<ButtonInput<KeyCode>>)
                    .run_if(resource_exists::<ButtonInput<MouseButton>>),
            )
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn on() -> Inputs {
        Inputs { on: true, grounded: true, ..Inputs::default() }
    }

    /// Frames until the share settles, with the inputs held.
    fn settle(p: &mut Prone, i: &Inputs, sneak: &mut bool) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..600 {
            let mut req = false;
            p.frame(i, sneak, &mut req, DT);
            out.push(p.value());
            if p.share.rate == 0.0 && (p.value() == 0.0 || p.value() == 1.0) {
                break;
            }
        }
        out
    }

    #[test]
    fn a_request_lies_down_and_keeps_sneaking() {
        let mut p = Prone::default();
        let (mut sneak, mut req) = (true, true);
        assert_eq!(p.frame(&on(), &mut sneak, &mut req, DT), Some(Change::Down));
        assert!(p.prone && sneak && !req, "the request is spent and sneaking kept");
        // From standing (Prone's own key): sneaking all the same.
        let mut q = Prone::default();
        let (mut sneak, mut req) = (false, true);
        assert_eq!(q.frame(&on(), &mut sneak, &mut req, DT), Some(Change::Down));
        assert!(sneak);
        // Lying, sneaking stays on frame after frame.
        let shares = settle(&mut q, &on(), &mut sneak);
        assert!(sneak && q.prone && *shares.last().unwrap() == 1.0);
    }

    #[test]
    fn nothing_happens_with_prone_off_or_on_the_move() {
        let mut p = Prone::default();
        let (mut sneak, mut req) = (false, true);
        assert_eq!(p.frame(&Inputs { on: false, ..on() }, &mut sneak, &mut req, DT), None);
        assert!(!p.prone && !sneak && !req && p.value() == 0.0, "off: as the original");
        for (i, why) in [(Inputs { moving: true, ..on() }, "moving"), (Inputs { grounded: false, ..on() }, "air"), (Inputs { swimming: true, ..on() }, "swim"), (Inputs { held: true, ..on() }, "seat"), (Inputs { locked: true, ..on() }, "lock")] {
            let mut req = true;
            assert_eq!(p.frame(&i, &mut sneak, &mut req, DT), None, "{why}");
            assert!(!p.prone, "{why}");
        }
        assert_eq!(p.refused, 5);
    }

    #[test]
    fn moving_stands_up_and_the_movement_waits() {
        let mut p = Prone::default();
        let (mut sneak, mut req) = (true, true);
        p.frame(&on(), &mut sneak, &mut req, DT);
        settle(&mut p, &on(), &mut sneak);
        let mv = Inputs { moving: true, ..on() };
        assert_eq!(p.frame(&mv, &mut sneak, &mut req, DT), Some(Change::Stand("moved")));
        assert!(!p.prone && !sneak, "they stand: no longer sneaking");
        // Hold to Sneak with the key still held: sneaking stays.
        let mut q = Prone { prone: true, ..Prone::default() };
        let mut s2 = true;
        q.frame(&Inputs { moving: true, hold_to_sneak: true, crouch_held: true, ..on() }, &mut s2, &mut false, DT);
        assert!(!q.prone && s2);
    }

    #[test]
    fn crouch_pressed_again_goes_back_to_the_crouch_and_its_hold_does_not_lie_down() {
        let mut p = Prone { prone: true, ..Prone::default() };
        // The shared key's press toggled sneaking off before this runs.
        let mut sneak = false;
        let c = p.frame(&Inputs { crouch_pressed: true, hold: 0.016, ..on() }, &mut sneak, &mut false, DT);
        assert_eq!(c, Some(Change::ToCrouch("Crouch/Sneak pressed")));
        assert!(sneak && !p.prone);
        // The same hold reaches a second: no lying down again.
        let mut req = true;
        assert_eq!(p.frame(&Inputs { hold: 1.0, ..on() }, &mut sneak, &mut req, DT), None);
        assert!(!p.prone && !req);
        // Let go, then a new hold lies down.
        p.frame(&Inputs { hold: 0.0, ..on() }, &mut sneak, &mut false, DT);
        let mut req = true;
        assert_eq!(p.frame(&Inputs { hold: 1.0, ..on() }, &mut sneak, &mut req, DT), Some(Change::Down));
        // Prone's own key pressed while lying: to the crouch too.
        let mut req = true;
        p.frame(&on(), &mut sneak, &mut false, DT);
        assert_eq!(p.frame(&Inputs { hold: 0.0, ..on() }, &mut sneak, &mut req, DT), Some(Change::ToCrouch("Prone asked again")));
        assert!(sneak);
    }

    #[test]
    fn a_lock_or_a_script_stands_up_and_hold_to_sneak_keeps_lying() {
        let mut p = Prone { prone: true, ..Prone::default() };
        let mut sneak = false;
        assert_eq!(p.frame(&Inputs { locked: true, ..on() }, &mut sneak, &mut false, DT), Some(Change::Stand("a script's sneaking lock")));
        assert!(!sneak);
        let mut p = Prone { prone: true, ..Prone::default() };
        assert_eq!(p.frame(&on(), &mut sneak, &mut false, DT), Some(Change::Stand("sneaking ended")));
        let mut p = Prone { prone: true, ..Prone::default() };
        let mut sneak = false;
        assert_eq!(p.frame(&Inputs { hold_to_sneak: true, crouch_released: true, ..on() }, &mut sneak, &mut false, DT), None);
        assert!(p.prone && sneak, "Hold to Sneak: letting go keeps lying");
    }

    #[test]
    fn the_ground_going_or_the_switch_going_off_goes_back_to_the_crouch() {
        let mut p = Prone { prone: true, ..Prone::default() };
        let mut sneak = true;
        let air = Inputs { grounded: false, ..on() };
        let mut changed = None;
        for _ in 0..30 {
            changed = changed.or(p.frame(&air, &mut sneak, &mut false, DT));
        }
        assert_eq!(changed, Some(Change::ToCrouch("the ground went")));
        assert!(sneak);
        let mut p = Prone { prone: true, ..Prone::default() };
        assert_eq!(p.frame(&Inputs { on: false, ..on() }, &mut sneak, &mut false, DT), Some(Change::ToCrouch("Prone switched off")));
        assert!(sneak && !p.prone);
    }

    #[test]
    fn the_share_eases_without_jumps_and_arrives_at_rest() {
        let mut p = Prone::default();
        let (mut sneak, mut req) = (true, true);
        p.frame(&on(), &mut sneak, &mut req, DT);
        let first = p.value();
        let down: Vec<f32> = std::iter::once(first).chain(settle(&mut p, &on(), &mut sneak)).collect();
        let speeds: Vec<f32> = std::iter::once(0.0).chain(down.iter().copied()).collect::<Vec<_>>().windows(2).map(|w| (w[1] - w[0]) / DT).collect();
        // Rising only, never past 1, a slow start and a slow end.
        assert!(down.windows(2).all(|w| w[1] >= w[0]) && down.iter().all(|v| *v <= 1.0));
        assert!(speeds[0] < 0.2 && speeds[speeds.len() - 2].abs() < 0.05, "{} … {}", speeds[0], speeds[speeds.len() - 2]);
        let t95 = down.iter().position(|v| *v >= 0.95).unwrap() as f32 * DT;
        assert!((t95 - DOWN_SECONDS).abs() < 0.05, "95 % in {t95}");
        // No frame changes its speed by much: the curve is smooth.
        assert!(speeds.windows(2).all(|w| (w[1] - w[0]).abs() < 0.3), "a jolt");
        // Getting up, half-way down again: the speed turns back without a jump.
        let mut q = Prone::default();
        let mut s = true;
        q.frame(&on(), &mut s, &mut true, DT);
        for _ in 0..30 {
            q.frame(&on(), &mut s, &mut false, DT);
        }
        let (v0, r0) = (q.value(), q.share.rate);
        q.frame(&Inputs { moving: true, ..on() }, &mut s, &mut false, DT);
        assert!((q.value() - v0).abs() < r0 * DT + 1e-3, "no jump of the value");
        let up = settle(&mut q, &on(), &mut s);
        assert_eq!(*up.last().unwrap(), 0.0);
        assert!(up.iter().all(|v| *v >= 0.0 && *v <= 1.0));
    }

    #[test]
    fn the_bonuses_scale_with_the_share_and_are_nothing_standing() {
        assert_eq!((spread_mult(0.0), recoil_mult(0.0), sway_mult(0.0), vats_bonus(0.0)), (1.0, 1.0, 1.0, 0.0));
        assert_eq!((spread_mult(1.0), recoil_mult(1.0), sway_mult(1.0), vats_bonus(1.0)), (SPREAD, RECOIL, SWAY, VATS_POINTS));
        assert!((spread_mult(0.5) - 0.75).abs() < 1e-6 && (vats_bonus(0.5) - 7.5).abs() < 1e-6);
        // Never past the flat values.
        assert_eq!(spread_mult(1.5), SPREAD);
        assert_eq!(pitch_floor(0.0), -MAX_PITCH);
        assert!((pitch_floor(1.0) + PITCH_DOWN_DEG.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn the_vats_bonus_stays_under_the_games_cap() {
        let t = fnv_combat::vats::HitTuning::default();
        let capped = fnv_combat::vats::final_chance(90.0, |c| c + vats_bonus(1.0), None, &t);
        assert_eq!(capped, t.max_chance);
        let low = fnv_combat::vats::final_chance(40.0, |c| c + vats_bonus(1.0), None, &t);
        assert_eq!(low, 55.0);
    }

    /// The whole path: a second's hold of Crouch/Sneak (`crate::control::read_controls`)
    /// crouches at once and lies down at the second; a movement key stands up, and the
    /// movement waits for the body; the scripts' sneaking lock stands up too.
    #[test]
    fn holding_crouch_lies_down_and_moving_gets_up() {
        use bevy::input::mouse::AccumulatedMouseScroll;
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::bindings::Bindings>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<PlayerControl>()
            .init_resource::<MoveIntent>()
            .init_resource::<ControlLocks>()
            .init_resource::<StanceMotion>()
            .init_resource::<Prone>()
            .insert_resource({
                let mut m = MovementInertia::default();
                m.prone = true;
                m
            })
            .insert_resource(Player { heading: 0.0, pitch: -1.2, eye_height: 112.0, feet: [0.0; 3], body_eye: None, body_view: None, placed: None })
            .init_resource::<Time>()
            .insert_state(PlayState::Playing)
            .add_systems(Update, (crate::control::read_controls, update).chain());
        app.world_mut().resource_mut::<crate::bindings::Bindings>().prone_on = true;
        let step = |app: &mut App| {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(DT));
            app.update();
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        };
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ControlLeft);
        step(&mut app);
        assert!(app.world().resource::<PlayerControl>().sneak, "crouched at the press");
        assert!(!app.world().resource::<StanceMotion>().prone);
        for _ in 0..61 {
            step(&mut app);
        }
        let m = *app.world().resource::<StanceMotion>();
        assert!(m.prone && m.prone_share > 0.0 && app.world().resource::<PlayerControl>().sneak, "lying after a second, still sneaking: {m:?}");
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::ControlLeft);
        for _ in 0..120 {
            step(&mut app);
        }
        let m = *app.world().resource::<StanceMotion>();
        assert!(m.prone && m.prone_share > 0.99 && app.world().resource::<PlayerControl>().sneak);
        assert!(app.world().resource::<MoveIntent>().sneak);
        assert!(app.world().resource::<Player>().pitch >= pitch_floor(m.prone_share) - 1e-6, "the view kept off the ground");
        // Forward: up, no longer sneaking, the movement held while the body is low.
        app.world_mut().resource_mut::<MoveIntent>().forward = 1.0;
        step(&mut app);
        let m = *app.world().resource::<StanceMotion>();
        assert!(!m.prone && !app.world().resource::<PlayerControl>().sneak);
        assert_eq!(app.world().resource::<MoveIntent>().forward, 0.0, "no crawl");
        let mut moved_at = None;
        for f in 0..200 {
            app.world_mut().resource_mut::<MoveIntent>().forward = 1.0;
            step(&mut app);
            if moved_at.is_none() && app.world().resource::<MoveIntent>().forward == 1.0 {
                moved_at = Some((f, app.world().resource::<StanceMotion>().prone_share));
            }
        }
        let (_, share) = moved_at.expect("moving again once up");
        assert!(share <= MOVE_FROM_SHARE);
        assert_eq!(app.world().resource::<StanceMotion>().prone_share, 0.0);
        // Down again, then the scripts' sneaking lock: up.
        app.world_mut().resource_mut::<MoveIntent>().forward = 0.0;
        app.world_mut().resource_mut::<PlayerControl>().prone_request = true;
        step(&mut app);
        assert!(app.world().resource::<StanceMotion>().prone);
        let mut c = *app.world().resource::<PlayerControl>();
        let mut i = *app.world().resource::<MoveIntent>();
        ControlLocks { sneaking: true, ..Default::default() }.apply(&mut i, &mut c);
        *app.world_mut().resource_mut::<PlayerControl>() = c;
        app.world_mut().resource_mut::<ControlLocks>().sneaking = true;
        step(&mut app);
        assert!(!app.world().resource::<StanceMotion>().prone && !app.world().resource::<PlayerControl>().sneak);
    }

    #[test]
    fn prone_off_leaves_the_controls_alone() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::bindings::Bindings>()
            .init_resource::<PlayerControl>()
            .init_resource::<MoveIntent>()
            .init_resource::<ControlLocks>()
            .init_resource::<StanceMotion>()
            .init_resource::<Prone>()
            .init_resource::<MovementInertia>()
            .insert_resource(Player { heading: 0.0, pitch: -1.5, eye_height: 112.0, feet: [0.0; 3], body_eye: None, body_view: None, placed: None })
            .init_resource::<Time>()
            .insert_state(PlayState::Playing)
            .add_systems(Update, update);
        app.world_mut().resource_mut::<PlayerControl>().prone_request = true;
        app.world_mut().resource_mut::<MoveIntent>().forward = 1.0;
        app.update();
        assert_eq!(*app.world().resource::<StanceMotion>(), StanceMotion::default());
        assert_eq!(app.world().resource::<MoveIntent>().forward, 1.0);
        assert_eq!(app.world().resource::<Player>().pitch, -1.5, "the view as the original's");
        assert!(!app.world().resource::<PlayerControl>().sneak);
    }
}
