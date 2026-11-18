//! Prone's look (`nvf_play::prone`, the shared state `nvf_play::stance::StanceMotion`): the
//! first-person view lowered to the ground and the weapon steadied, the third-person body
//! laid down with the game's push-up clip [design: the owner 2026-11-18; Advanced Movement's
//! animation rules, docs/design/player-body.md §1.4a]. `player_body.rs` calls in at a few
//! marked places; everything blends by `StanceMotion::prone_share` (a critically damped
//! spring, `nvf_play::prone`), so lying down and getting up are one smooth motion each way and
//! a change of mind part way never jumps.
//!
//! **First person** (no lying clip exists for the arms rig: the first-person lying clips are
//! all face-up wake-ups [observed: the research of 2026-11-18]): the whole arms rig, the
//! camera node with it, goes down from the crouch's eye ([`CROUCH_EYE`]) to [`PRONE_EYE`]
//! over the floor by the share ([`eye_drop`]), and never closer to the ground under the eye
//! than [`MIN_CLEARANCE`] (on a slope the ground ahead rises: [`Clearance`], on a short
//! spring). The arms sway and the head bobs and breathes less ([`ARMS_STEADY`],
//! [`HEAD_STEADY`]); off the sights the gun rests a little lower and nearer, its muzzle tipped
//! down ([`rest_pose`]); it aims as ever. `nvf_play::prone` keeps the view from looking
//! straight down into the ground.
//!
//! **Third person**: `meshes\characters\_male\idleanims\pushups.kf` (`SpecialIdle_PushUps`,
//! 8 s: 0–1.67 s standing to a plank, a push-up loop to 6.67 s, back to standing by 8 s; no
//! root motion sideways, the body centred on its root [observed: the clip's text keys and
//! its pelvis and head sampled, 2026-11-18]) on the body over everything else:
//! - lying down: from 0.5 s (bent forward, the pelvis about a crouch's height) to the plank
//!   at 1.67 s, then from the loop's end (6.67 s, the same plank) back to the bottom of its
//!   last push-up at [`HOLD`] (6.17 s: pelvis 14, head 11 units up, chest down, hands under
//!   the shoulders), held there with a slow breath of our own ([`BREATH_CLIP`]; the clip has
//!   no face-down breathing loop);
//! - getting up: the same clip on from [`HOLD`]: the push up to the plank and the clip's own
//!   getting up (6.67–8.0 s, the motion `SpecialIdle_PushUpsGetUp` / `pushupsgetup.kf` has
//!   too, from the same plank). Chosen over `mt_knockdownfacedown.kf` (the player's knockdown
//!   get-up): that one starts flat on the face with its root 68 units off and rising 58,
//!   which would have to be placed against the held pose and blended across a different
//!   lying pose; the push-up's own way up starts from exactly the held pose, in place;
//! - the clip's time follows the share, so it goes at the share's spring; going down and
//!   getting up are two layers of it, crossing over on a short spring ([`TURN_SECONDS`]) when
//!   the player changes their mind; together they fade in over the crouch and out into the
//!   standing or crouching clips over the share's first [`BLEND_SHARE`] (the clip's tail
//!   hands over to what follows while it still moves);
//! - the clip holds no weapon (the hands flat on the floor): the drawn weapon is hidden in
//!   third person while the body is mostly down ([`WEAPON_HIDE_SHARE`]) and shots leave the
//!   eye; a weapon-aiming arm layer over the lying body would be new work (follow-up);
//! - the third-person camera's pivot stays at least [`PIVOT_UP`] over the feet by the share,
//!   above the lying body.

use crate::view_sway::SwayPose;
use bevy::prelude::*;
use nvf_assets::anim_blend::{settle, Animator, Mode};
use nvf_assets::anim_play::Clip;
use nvf_play::ease::{omega_for, smoothstep, Spring};
use std::sync::Arc;

/// The first-person eye flat on the ground, units over the floor [design: the owner's brief,
/// "about 15–20 units above the floor"].
pub const PRONE_EYE: f32 = 17.0;
/// The first-person camera node crouched, units over the feet [observed: the arms rig's
/// `Camera1st` 78 up crouched, player_body.rs's notes].
pub const CROUCH_EYE: f32 = 78.0;
/// The lowered eye is never closer than this to the ground under it [design].
pub const MIN_CLEARANCE: f32 = 12.0;
/// The lift that keeps the eye off rising ground follows on a spring of this time (95 %)
/// [design].
pub const CLEARANCE_SECONDS: f32 = 0.2;
/// The arms' sway flat on the ground: this share of it [design: "less sway/breathing"].
pub const ARMS_STEADY: f32 = 0.5;
/// The head's bob, breath and nudges flat on the ground: this share [design].
pub const HEAD_STEADY: f32 = 0.5;
/// Off the sights, flat on the ground, the gun rests this much lower and nearer (units) and
/// its muzzle this much down (degrees) [design: "a steadier, slightly lowered/resting pose"].
pub const GUN_REST_DOWN: f32 = 1.2;
pub const GUN_REST_BACK: f32 = 0.6;
pub const GUN_REST_TIP_DEG: f32 = 2.0;
/// The third-person camera's pivot flat on the ground: at least this far over the feet
/// [design: the head is 11 up].
pub const PIVOT_UP: f32 = 45.0;

/// The push-up clip, under the body's skeleton folder.
pub const CLIP: &str = "idleanims\\pushups.kf";
/// Its times (seconds into it) [observed: its text keys `StartLoop` 1.67, `EndLoop` 6.67,
/// `end` 8.0; the pelvis and head sampled, 2026-11-18]: where lying down starts (bent
/// forward, pelvis 43), the plank, the loop's end (the same plank), the bottom held, the end.
pub const FROM: f32 = 0.5;
pub const PLANK: f32 = 1.67;
pub const LOOP_END: f32 = 6.67;
pub const HOLD: f32 = 6.17;
pub const END: f32 = 8.0;
/// The share of the way down the clip reaches the plank: by the clip's own times, so its two
/// parts run at one pace.
pub const PLANK_SHARE: f32 = (PLANK - FROM) / ((PLANK - FROM) + (LOOP_END - HOLD));
/// The clip fades in over the share's first this much going down, and out over its last
/// getting up [design].
pub const BLEND_SHARE: f32 = 0.3;
/// The going-down and getting-up layers cross over on a spring of this time [design].
pub const TURN_SECONDS: f32 = 0.35;
/// The breath lying down: one every this many seconds, moving the clip this far back up
/// from [`HOLD`] (the chest lifts a little) [design].
pub const BREATH_SECONDS: f32 = 4.0;
pub const BREATH_CLIP: f32 = 0.05;
/// In third person the drawn weapon is hidden while the share is over this [design].
pub const WEAPON_HIDE_SHARE: f32 = 0.35;
/// The layers' sequence priority: over every clip of the body, as a script's sequence
/// [design].
pub const PRIORITY: i32 = 100;
/// The layers' roles (`Animator` tags), going down and getting up.
pub const TAG_DOWN: u32 = 0x2000_0000;
pub const TAG_UP: u32 = 0x2000_0001;

/// The push-up clip's time going down at `share` (0 crouched, 1 flat).
pub fn down_time(share: f32) -> f32 {
    let s = share.clamp(0.0, 1.0);
    if s <= PLANK_SHARE {
        FROM + (PLANK - FROM) * s / PLANK_SHARE
    } else {
        LOOP_END - (LOOP_END - HOLD) * (s - PLANK_SHARE) / (1.0 - PLANK_SHARE)
    }
}

/// The push-up clip's time getting up at `share` (1 flat, 0 standing).
pub fn up_time(share: f32) -> f32 {
    HOLD + (END - HOLD) * (1.0 - share.clamp(0.0, 1.0))
}

/// How much of the body the clip has at `share`: in along the settling curve over the
/// share's first [`BLEND_SHARE`].
pub fn blend(share: f32) -> f32 {
    settle((share / BLEND_SHARE).clamp(0.0, 1.0))
}

/// How far the first-person rig goes down at `share`, for a body of `scale`.
pub fn eye_drop(share: f32, scale: f32) -> f32 {
    share.clamp(0.0, 1.0) * (CROUCH_EYE * scale - PRONE_EYE).max(0.0)
}

/// The arms' sway steadied at `share`, with the gun's resting pose off the sights (`sights`:
/// how far up the sights are, 0–1), in the view's frame about the eye.
pub fn steady(p: SwayPose, share: f32, sights: f32) -> SwayPose {
    let s = share.clamp(0.0, 1.0);
    if s <= 0.0 {
        return p;
    }
    let k = 1.0 + (ARMS_STEADY - 1.0) * s;
    let scaled = SwayPose { rotation: Quat::from_scaled_axis(p.rotation.to_scaled_axis() * k), offset: p.offset * k };
    crate::weapon_lower::then(scaled, rest_pose(s * (1.0 - sights.clamp(0.0, 1.0))))
}

/// The gun resting by `share` (0–1): lower, nearer, its muzzle tipped down.
pub fn rest_pose(share: f32) -> SwayPose {
    if share <= 0.0 {
        return SwayPose::NONE;
    }
    SwayPose { rotation: Quat::from_rotation_x(-GUN_REST_TIP_DEG.to_radians() * share), offset: Vec3::new(0.0, -GUN_REST_BACK * share, -GUN_REST_DOWN * share) }
}

/// The head's bob and breath steadied at `share`.
pub fn head_share(share: f32) -> f32 {
    1.0 + (HEAD_STEADY - 1.0) * share.clamp(0.0, 1.0)
}

/// The lift keeping the lowered eye off the ground (units), on a short spring.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Clearance {
    pub lift: Spring,
}

impl Clearance {
    /// `dt` seconds towards what keeps an eye that would be at `eye_z` at least
    /// [`MIN_CLEARANCE`] over `ground_z` (none known: no lift).
    pub fn step(&mut self, eye_z: f32, ground_z: Option<f32>, dt: f32) -> f32 {
        let need = ground_z.map_or(0.0, |g| (g + MIN_CLEARANCE - eye_z).max(0.0));
        self.lift.step(need, omega_for(CLEARANCE_SECONDS), dt);
        self.lift.value
    }
}

/// The push-up layers on the third-person body this frame: the clip and, going down and
/// getting up, each layer's time and share of the body.
#[derive(Clone)]
pub struct ProneLayers {
    pub clip: Arc<Clip>,
    pub down: (f32, f32),
    pub up: (f32, f32),
}

/// What the look keeps across frames (in `player_body.rs`'s motion state).
#[derive(Debug, Clone, Default)]
pub struct ProneLook {
    /// 0 the going-down layer, 1 the getting-up one.
    turn: Spring,
    /// The breath's clock while flat.
    breath: f32,
    /// The first-person rig's eye over its root last frame, before the drop (units).
    pub eye_up: Option<f32>,
    pub clearance: Clearance,
    /// Logged: the clip missing, the weapon hidden, the eye flat.
    pub told_missing: bool,
    pub weapon_hidden: bool,
    pub told_flat: bool,
}

impl ProneLook {
    /// The two layers' (time, share) at `share` with the player `prone` (lying is the target),
    /// after `dt` seconds.
    pub fn layers(&mut self, prone: bool, share: f32, dt: f32) -> [(f32, f32); 2] {
        if share <= 0.0 {
            // Up (or never down): the next time starts going down, at once and unseen.
            self.turn = Spring::REST;
            self.breath = 0.0;
            return [(down_time(0.0), 0.0), (up_time(0.0), 0.0)];
        }
        self.turn.step(f32::from(u8::from(!prone)), omega_for(TURN_SECONDS), dt);
        let r = self.turn.value.clamp(0.0, 1.0);
        // A slow breath, flat on the ground only, faded in near the bottom.
        let flat = smoothstep(0.9, 1.0, share);
        if prone && flat > 0.0 {
            self.breath += dt;
        }
        let breath = BREATH_CLIP * flat * 0.5 * (1.0 - (std::f32::consts::TAU * self.breath / BREATH_SECONDS).cos());
        let w = blend(share);
        [(down_time(share) + breath, w * (1.0 - r)), (up_time(share) + breath, w * r)]
    }
}

/// Play the push-up layers on `a` (between its `begin` and `end`), or drop them.
pub fn pose_layers(a: &mut Animator, layers: Option<&ProneLayers>) {
    let Some(p) = layers else {
        a.cut(TAG_DOWN);
        a.cut(TAG_UP);
        return;
    };
    for (tag, (time, ease)) in [(TAG_DOWN, p.down), (TAG_UP, p.up)] {
        if ease <= 0.0 {
            a.cut(tag);
            continue;
        }
        // Its fade is the share given here, not a timer's (an endless fade-in keeps it).
        let i = a.want(&p.clip, tag, 1.0, Mode::Once, f32::INFINITY);
        let l = &mut a.layers[i];
        (l.rate, l.time, l.ease, l.sequence_priority) = (0.0, time, ease.min(1.0), PRIORITY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clip_goes_down_to_the_held_bottom_and_up_to_standing_in_one_line() {
        assert_eq!(down_time(0.0), FROM);
        assert!((down_time(PLANK_SHARE) - PLANK).abs() < 1e-4);
        // Past the plank it carries on from the loop's end, the same pose, to the bottom.
        assert!((down_time(PLANK_SHARE + 1e-4) - LOOP_END).abs() < 0.01);
        assert!((down_time(1.0) - HOLD).abs() < 1e-5);
        assert!((up_time(1.0) - HOLD).abs() < 1e-5 && (up_time(0.0) - END).abs() < 1e-5);
        // The two parts at one pace: the clip's seconds per share alike.
        let a = (PLANK - FROM) / PLANK_SHARE;
        let b = (LOOP_END - HOLD) / (1.0 - PLANK_SHARE);
        assert!((a - b).abs() < 1e-3, "{a} {b}");
    }

    #[test]
    fn the_layers_fade_in_smoothly_and_cross_over_without_a_jump() {
        assert_eq!(blend(0.0), 0.0);
        assert_eq!(blend(BLEND_SHARE), 1.0);
        let mut look = ProneLook::default();
        let dt = 1.0 / 60.0;
        // Going down: only the down layer, held at the bottom with a small breath.
        let [d, u] = look.layers(true, 1.0, dt);
        assert!((d.0 - HOLD).abs() <= BREATH_CLIP && d.1 == 1.0 && u.1 == 0.0);
        let mut last = d;
        for _ in 0..240 {
            let [d, _] = look.layers(true, 1.0, dt);
            assert!((d.0 - last.0).abs() < 0.01, "the breath is slow");
            assert!(d.0 >= HOLD - 1e-6 && d.0 <= HOLD + BREATH_CLIP + 1e-6);
            last = d;
        }
        // Getting up: the up layer takes over from the same pose, the total unchanged.
        let [d0, u0] = look.layers(false, 1.0, dt);
        assert!((d0.1 + u0.1 - 1.0).abs() < 1e-5 && u0.1 < 0.1, "{d0:?} {u0:?}");
        assert!((d0.0 - u0.0).abs() < 1e-5, "both layers at the held pose");
        let mut prev = u0.1;
        for _ in 0..60 {
            let [_, u] = look.layers(false, 1.0, dt);
            assert!(u.1 >= prev && u.1 - prev < 0.1);
            prev = u.1;
        }
        // At rest standing: nothing shown, the next lying down starts with its own layer.
        let [d, u] = look.layers(false, 0.0, dt);
        assert_eq!((d.1, u.1), (0.0, 0.0));
        let [d, u] = look.layers(true, 0.05, dt);
        assert!(d.1 > 0.0 && u.1 == 0.0);
    }

    #[test]
    fn the_eye_goes_down_by_the_share_and_keeps_off_the_ground() {
        assert_eq!(eye_drop(0.0, 1.0), 0.0);
        assert!((eye_drop(1.0, 1.0) - (CROUCH_EYE - PRONE_EYE)).abs() < 1e-5);
        assert!((eye_drop(0.5, 1.0) - 0.5 * (CROUCH_EYE - PRONE_EYE)).abs() < 1e-5);
        // A small body never goes up.
        assert_eq!(eye_drop(1.0, 0.1), 0.0);
        // Ground rising ahead: lifted to the clearance on a short spring, never jumping.
        let mut c = Clearance::default();
        let dt = 1.0 / 60.0;
        let mut last = 0.0;
        for _ in 0..60 {
            let l = c.step(10.0, Some(5.0), dt);
            assert!(l >= last && l - last < 1.5);
            last = l;
        }
        assert!((last - 7.0).abs() < 0.1, "{last}");
        // Clear of the ground: the lift goes again.
        for _ in 0..120 {
            last = c.step(30.0, Some(0.0), dt);
        }
        assert!(last.abs() < 0.01);
        assert_eq!(Clearance::default().step(0.0, None, dt), 0.0);
    }

    #[test]
    fn the_gun_rests_and_steadies_by_the_share_and_not_down_the_sights() {
        let p = SwayPose { rotation: Quat::from_rotation_z(0.1), offset: Vec3::new(1.0, 0.0, -1.0) };
        assert_eq!(steady(p, 0.0, 0.0), p);
        let s = steady(p, 1.0, 1.0);
        assert!((s.offset - p.offset * ARMS_STEADY).length() < 1e-5, "down the sights: steadier, no rest pose");
        let r = steady(SwayPose::NONE, 1.0, 0.0);
        assert!(r.offset.z < 0.0 && r.offset.y < 0.0, "lower and nearer: {r:?}");
        assert_eq!(rest_pose(0.0), SwayPose::NONE);
        assert_eq!(head_share(0.0), 1.0);
        assert_eq!(head_share(1.0), HEAD_STEADY);
    }
}
