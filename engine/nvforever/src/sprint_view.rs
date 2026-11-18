//! Sprint's look (NVF Features › Advanced Movement › Sprint, `nvf_play::sprint`) [design:
//! the owner 2026-11-18; every constant an engineering choice, to be judged by eye]. All of
//! it follows `nvf_play::stance::StanceMotion::sprint_share`, which comes and goes on a
//! spring, and `StanceMotion::winded`, so none of it steps.
//!
//! - **The gun lowered** ([`pose`]): in first person a sprint turns the arms rig about the
//!   eye as the Lower Weapon pose does (`weapon_lower.rs`), further: the muzzle down, turned
//!   in across the body and canted, a little lower and nearer; heavy weapons and launchers
//!   less, rifles a little less ([`crate::weapon_lower::weight`]), fists [`FISTS`] of it.
//!   Part of Sprint itself (its switch); it fades through [`SprintView::pose_share`] where
//!   the arms are not the player's (the Pip-Boy, an idle).
//! - **The head bob** (Camera Motion, `head_bob.rs`): up to [`HEAD_BOB`] times the run's at
//!   a full sprint.
//! - **The gun's bob** (Weapon Inertia, `view_sway.rs`): up to [`WEAPON_BOB`] times the
//!   run's.
//! - **Out of breath** (Camera Motion, `head_bob.rs`): `StanceMotion::winded` drives the
//!   existing out-of-breath breathing (`head_bob::HeadBob::exertion`, as running winds the
//!   player): the view's rise and fall and the gun's breathing (Weapon Inertia) come faster
//!   and larger, up to `view_sway::EXERTED_PACE` and `EXERTED_SIZE` times, and on top of that
//!   [`WINDED_PACE`] and [`WINDED_SIZE`] times for a sprint's panting. Its place only, as the
//!   existing breath: the view's rotation is never touched.
//! - **Third person** ([`lean`]): the body leans forward up to [`LEAN`] about its feet. The
//!   run clip plays faster by itself: the movement clips play at the body's real speed over
//!   theirs (`player_body`, up to 2.5 times); no sprint clip exists (upstream closed #2176:
//!   the data has none).

use crate::view_sway::SwayPose;
use bevy::math::{Quat, Vec3};
use nvf_play::ease::{omega_for, Spring};

/// The head bob at a full sprint, times the run's (Camera Motion).
pub const HEAD_BOB: f32 = 1.4;
/// The gun's bob at a full sprint, times the run's (Weapon Inertia).
pub const WEAPON_BOB: f32 = 1.6;
/// Fully out of breath after a sprint the breathing comes this many times faster again, on
/// top of the existing out-of-breath pace (about 1.3 s a breath in all), and this many times
/// larger again (the view rises and falls about 2.9 units, 4 cm: clearly more than a walk's
/// bob of 1.5 units, less than a run's 5.25).
pub const WINDED_PACE: f32 = 1.6;
pub const WINDED_SIZE: f32 = 1.1;
/// The sprint pose at a full sprint about the eye: the muzzle down, turned in (left), canted
/// (top to the left), radians, and moved (x right, y ahead, z up), units. The Lower Weapon
/// pose is -9°, 10° and -5° (`weapon_lower.rs`).
pub const POSE_PITCH: f32 = -14.0 * std::f32::consts::PI / 180.0;
pub const POSE_TURN_IN: f32 = 16.0 * std::f32::consts::PI / 180.0;
pub const POSE_CANT: f32 = -14.0 * std::f32::consts::PI / 180.0;
pub const POSE_MOVE: Vec3 = Vec3::new(0.0, -1.0, -1.5);
/// The share of the pose the fists take.
pub const FISTS: f32 = 0.5;
/// The pose's own fade where the arms are not the player's (the Pip-Boy, an idle): a spring
/// settling in this many seconds.
pub const POSE_GATE_SECONDS: f32 = 0.3;
/// The third-person body's forward lean at a full sprint, radians (4°).
pub const LEAN: f32 = 4.0 * std::f32::consts::PI / 180.0;

/// How much larger the head bob is at a sprint share (0–1).
pub fn head_bob_gain(share: f32) -> f32 {
    1.0 + (HEAD_BOB - 1.0) * share.clamp(0.0, 1.0)
}

/// How much larger the gun's bob is at a sprint share (0–1).
pub fn weapon_bob_gain(share: f32) -> f32 {
    1.0 + (WEAPON_BOB - 1.0) * share.clamp(0.0, 1.0)
}

/// The breathing's pace and size for out of breath after a sprint (0–1), on top of the
/// existing out-of-breath's.
pub fn winded_pace(winded: f32) -> f32 {
    1.0 + (WINDED_PACE - 1.0) * winded.clamp(0.0, 1.0)
}

pub fn winded_size(winded: f32) -> f32 {
    1.0 + (WINDED_SIZE - 1.0) * winded.clamp(0.0, 1.0)
}

/// The arms' sprint pose at `share` (0–1, already eased) of `weight` (the weapon's share of
/// it: [`crate::weapon_lower::weight`], or [`FISTS`]), in the view's frame about the eye.
pub fn pose(share: f32, weight: f32) -> SwayPose {
    let s = share.clamp(0.0, 1.0) * weight;
    if s <= 0.0 {
        return SwayPose::NONE;
    }
    SwayPose { rotation: Quat::from_rotation_z(POSE_TURN_IN * s) * Quat::from_rotation_x(POSE_PITCH * s) * Quat::from_rotation_y(POSE_CANT * s), offset: POSE_MOVE * s }
}

/// The third-person body's lean at a sprint share, as a turn of the body about its feet
/// (in the body's own frame: x right, y ahead, z up), after its heading.
pub fn lean(share: f32) -> Quat {
    Quat::from_rotation_x(-LEAN * share.clamp(0.0, 1.0))
}

/// The first-person sprint pose's state across frames.
#[derive(Debug, Clone, Copy, Default)]
pub struct SprintView {
    gate: Spring,
}

impl SprintView {
    /// The share of the pose shown: the sprint's share while the arms are the player's
    /// (`on`), faded through a spring where they are not.
    pub fn pose_share(&mut self, share: f32, on: bool, dt: f32) -> f32 {
        self.gate.step(f32::from(u8::from(on)), omega_for(POSE_GATE_SECONDS), dt);
        share.clamp(0.0, 1.0) * self.gate.value.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::head_bob::{BobInput, HeadBob};
    use bevy::math::Vec2;

    const DT: f32 = 1.0 / 60.0;

    fn bob(speed: f32, phase: f32) -> BobInput {
        BobInput { dt: DT, speed, phase, heading: 0.0, sights: false, sneaking: false, airborne: false, velocity: Vec2::ZERO, vertical: 0.0, out_of_breath: true, hard_landings: true, landing: true, on: true, breathing: true, kick: 0.0, punch: Vec3::ZERO, sprint: 0.0, winded: 0.0 }
    }

    /// The view's rise and fall over `seconds` after `warm` seconds (peak to peak, units)
    /// and its largest change in a frame.
    fn swing(b: &mut HeadBob, i: impl Fn(usize) -> BobInput, warm: f32, seconds: f32) -> (f32, f32) {
        let n = ((warm + seconds) / DT) as usize;
        let zs: Vec<f32> = (0..n).map(|f| b.first_person(i(f)).z).collect();
        let late = &zs[(warm / DT) as usize..];
        let (lo, hi) = late.iter().fold((f32::MAX, f32::MIN), |(l, h), z| (l.min(*z), h.max(*z)));
        (hi - lo, zs.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max))
    }

    #[test]
    fn out_of_breath_shakes_the_view_more_than_a_walk_but_gently() {
        // A walk's bob (86.8 units/s, a stride a second).
        let (walk, _) = swing(&mut HeadBob::default(), |f| BobInput { breathing: false, ..bob(86.8, (f as f32 * DT).fract()) }, 2.0, 4.0);
        // Standing winded: fully and at the least a stop low on AP gives.
        for (w, more) in [(1.0, 2.5), (nvf_play::sprint::WINDED_LEAST, 1.5)] {
            let (winded, step) = swing(&mut HeadBob::default(), |_| BobInput { winded: w, ..bob(0.0, 0.0) }, 2.0, 4.0);
            assert!(winded > more * walk, "winded {w}: {winded} against a walk's {walk}");
            assert!(winded < crate::head_bob::DIP * 1.3, "gentle, about a run's bob at most: {winded}");
            assert!(step < 0.4, "smooth: {step} units in a frame at most");
        }
        // Faster than the calm breath: more rises in the same time.
        let rises = |w: f32| {
            let mut b = HeadBob::default();
            let zs: Vec<f32> = (0..(9.0 / DT) as usize).map(|_| b.first_person(BobInput { winded: w, ..bob(0.0, 0.0) }).z).collect();
            zs.windows(3).filter(|x| x[1] > x[0] && x[1] >= x[2]).count()
        };
        assert!(rises(1.0) >= 3 * rises(0.0), "{} against {}", rises(1.0), rises(0.0));
        // Camera Motion off: the breathing camera does not show, whatever the wind.
        let (off, _) = swing(&mut HeadBob::default(), |_| BobInput { winded: 1.0, breathing: false, on: false, ..bob(0.0, 0.0) }, 2.0, 4.0);
        assert!(off < 1e-4, "{off}");
    }

    #[test]
    fn a_sprint_bobs_the_head_more_than_a_run() {
        let run = |sprint: f32| swing(&mut HeadBob::default(), |f| BobInput { breathing: false, sprint, ..bob(344.0, (f as f32 * DT / 0.7).fract()) }, 2.0, 3.0).0;
        let (r, s) = (run(0.0), run(1.0));
        assert!(s > 1.25 * r && s < HEAD_BOB * r * 1.05, "{s} against {r}");
    }

    #[test]
    fn the_pose_lowers_the_gun_by_the_share_and_fades_where_the_arms_are_not_the_players() {
        assert_eq!(pose(0.0, 1.0), SwayPose::NONE);
        let full = pose(1.0, 1.0);
        let half = pose(0.5, 1.0);
        let muzzle = |p: SwayPose| p.rotation * Vec3::Y;
        assert!(muzzle(full).z < muzzle(half).z && muzzle(half).z < 0.0, "the muzzle down, more the fuller");
        assert!(muzzle(full).x < 0.0, "turned in across the body");
        assert!(full.offset.z < 0.0);
        assert!(pose(1.0, 0.35).rotation.angle_between(Quat::IDENTITY) < full.rotation.angle_between(Quat::IDENTITY));
        let mut v = SprintView::default();
        let on: Vec<f32> = (0..60).map(|_| v.pose_share(1.0, true, DT)).collect();
        assert!(on.windows(2).all(|w| w[1] >= w[0]) && on[59] > 0.99);
        let off: Vec<f32> = (0..60).map(|_| v.pose_share(1.0, false, DT)).collect();
        assert!(off.windows(2).all(|w| w[1] <= w[0] && w[0] - w[1] < 0.15) && off[59] < 0.01, "fades, never pops");
        // The lean tips the body's up ahead.
        assert!((lean(1.0) * Vec3::Z).y > 0.05 && lean(0.0) == Quat::IDENTITY);
    }
}
