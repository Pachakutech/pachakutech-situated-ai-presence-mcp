//! Screen aim for the sitting figure.
//!
//! Theta is a clock on the screen: 0 below her, +π/2 to the viewer's right,
//! π above, −π/2 to the viewer's left. That angle picks a frame of `Point`.
//! Frames 1–150 raise her left hand and frames 210–360 raise her right, so
//! the viewer's right is frames 1–180 and a talk with no mark is frames 1–20.
//! Root yaw comes from the commanded angle, not from the frame the arms are
//! passing through, so a raise from the low pose to straight up does not yaw
//! the body out to the side and back. The camera's Ry(π) is the half turn
//! that faces her. The root does not add another. Yaw is negated so that
//! mirrored X still puts her left hand on the viewer's right. Two critically
//! damped springs carry the pose clock and the yaw. An exact tie between the
//! high pose and the low pose takes the low pose.
//!
//! A highlight is a wave between two corners. Those are the two nearest when
//! they tie, and otherwise the two that are neither nearest nor farthest.
//! The farthest corner lies between that pair. She starts in the middle of
//! the arc, and that wave still turns her with it. Talking with no mark is
//! separate: the arms go out to frame 20, her left hand, and back to the low
//! pose, and the body only yaws a couple of degrees the other way.

/// Left at 0. `body_camera_dq` is already Ry(π) and brings mesh +Z toward
/// the viewer. A second half turn on the root showed her back. Do not put π
/// back here, and do not rotate about Z.
pub const FACING_YAW: f32 = 0.0;

/// One breath. The 15 s clip is played through in this many seconds.
pub const BREATH_PERIOD_S: f32 = 4.0;

/// Natural frequency of the aim spring, in rad/s. Critically damped, so a
/// side-to-side turn settles in about five seconds.
pub const AIM_OMEGA: f32 = 0.8;

/// Arm joints copied from `Point` onto the breath. Spine stays on `Breathe`.
pub const ARM_JOINTS: &[&str] = &[
    "upperarm01.L",
    "upperarm02.L",
    "lowerarm01.L",
    "lowerarm02.L",
    "wrist.L",
    "upperarm01.R",
    "upperarm02.R",
    "lowerarm01.R",
    "lowerarm02.R",
    "wrist.R",
];

/// Wrap into (−π, π].
pub fn wrap_pi(a: f32) -> f32 {
    let mut x = a.rem_euclid(std::f32::consts::TAU);
    if x > std::f32::consts::PI {
        x -= std::f32::consts::TAU;
    }
    x
}

/// Shortest signed step from `from` to `to`. A half-turn tie prefers the arc
/// whose middle is closer to theta 0, the arms-down pose.
pub fn shortest_delta(from: f32, to: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let raw = (to - from).rem_euclid(std::f32::consts::TAU);
    let d = if raw > pi { raw - std::f32::consts::TAU } else { raw };
    if (d.abs() - pi).abs() < 1e-3 {
        let via = |dir: f32| wrap_pi(from + dir * 0.5).abs();
        if via(-pi) < via(pi) { -pi } else { pi }
    } else {
        d
    }
}

/// Screen y grows down. `atan2(dx, dy)`: 0 below, +π/2 to the viewer's right.
pub fn aim_theta(figure_x: f32, figure_y: f32, box_x: f32, box_y: f32) -> f32 {
    let dx = box_x - figure_x;
    let dy = box_y - figure_y;
    dx.atan2(dy)
}

/// The four corners of a highlight, in screen pixels. Screen y grows down.
pub fn box_corners(u: f32, v: f32, w: f32, h: f32, screen_w: f32, screen_h: f32) -> [(f32, f32); 4] {
    let x0 = u * screen_w;
    let y0 = v * screen_h;
    let x1 = (u + w) * screen_w;
    let y1 = (v + h) * screen_h;
    [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
}

/// `Point` time for this theta, wrapped into `[0, duration)`. One turn of the
/// clock is one play of the clip: theta 0 is the low pose, theta π/2 is a
/// quarter of the way through. A 15 s export puts straight up at 7.5 s. A
/// 360 s export, one second per degree, puts it at 180 s with no other change.
pub fn point_time(theta: f32, duration: f32) -> f32 {
    if duration <= 0.0 {
        return 0.0;
    }
    (theta / std::f32::consts::TAU).rem_euclid(1.0) * duration
}

/// Yaw about +Y from the same theta. 0 above and below. The camera's Ry(π)
/// mirrors skeleton +X onto the viewer's left, so a target on the viewer's
/// right returns −π/2 and a target on the viewer's left returns +π/2. The
/// raised hand then sits on the same side as the target. `FACING_YAW` stays 0.
pub fn aim_yaw(theta: f32) -> f32 {
    -theta.sin().atan2(theta.cos().abs())
}

/// Hold the middle of a highlight arc this long before the wave leaves it.
/// The aim spring settles in about five seconds, so the hold matches.
pub const SWEEP_SETTLE_S: f32 = 5.0;

/// One trip out to a corner and across to the other.
pub const SWEEP_PERIOD_S: f32 = 15.0;

/// One trip out and back while talking with no mark.
pub const TALK_WAVE_PERIOD_S: f32 = 15.0;

/// Frame 20 of the 360-frame `Point` clip. Frames 1–150 raise her left hand,
/// so this trip is that hand. Facing the viewer, it sits on the viewer's
/// right. Frame 340 is her right hand, on the viewer's left.
pub const TALK_WAVE_RAD: f32 = 20.0 * std::f32::consts::PI / 180.0;

/// Body yaw on that trip. The arms take the clip frames; the turn stays small.
pub const TALK_YAW_RAD: f32 = 2.0 * std::f32::consts::PI / 180.0;

/// Ends of the wave, as angles.
///
/// Sort the corners by distance from the figure. A tie of the two nearest
/// (the near edge of a box straight above) makes those two the ends.
/// Otherwise the ends are the two that are neither nearest nor farthest, and
/// the arc is the one with the farthest corner between them. `start` to `end`
/// runs in the direction of increasing angle.
pub fn wave_endpoints(figure_x: f32, figure_y: f32, corners: [(f32, f32); 4]) -> (f32, f32) {
    let mut order = [0usize, 1, 2, 3];
    let dist = corners.map(|(x, y)| {
        let dx = x - figure_x;
        let dy = y - figure_y;
        dx * dx + dy * dy
    });
    order.sort_by(|&i, &j| dist[i].total_cmp(&dist[j]).then(i.cmp(&j)));
    let d = |i: usize| dist[order[i]].sqrt();
    let tied = |near: f32, far: f32| far <= near * (1.0 + 1e-3) + 1e-2;
    let theta = |i: usize| {
        let (x, y) = corners[order[i]];
        aim_theta(figure_x, figure_y, x, y)
    };
    if tied(d(0), d(1)) && !tied(d(1), d(2)) {
        return arc_through(theta(0), theta(1), theta(3));
    }
    if !tied(d(0), d(1)) {
        return arc_through(theta(1), theta(2), theta(3));
    }
    // She is inside the box: three or four corners are the same distance.
    // The short arc that contains every corner is the only wedge left.
    let mut ang = corners.map(|(x, y)| aim_theta(figure_x, figure_y, x, y));
    ang.sort_by(|a, b| a.total_cmp(b));
    let tau = std::f32::consts::TAU;
    let mut best_gap = -1.0_f32;
    let mut best_i = 0_usize;
    for i in 0..4 {
        let next = if i + 1 == 4 { ang[0] + tau } else { ang[i + 1] };
        let gap = next - ang[i];
        if gap > best_gap {
            best_gap = gap;
            best_i = i;
        }
    }
    let start = if best_i + 1 == 4 { ang[0] } else { ang[best_i + 1] };
    (start, ang[best_i])
}

/// The arc from `a` to `b` that contains `far`. Increasing angle from the
/// returned start reaches `far` before the end.
fn arc_through(a: f32, b: f32, far: f32) -> (f32, f32) {
    let tau = std::f32::consts::TAU;
    let span = (b - a).rem_euclid(tau);
    let along = (far - a).rem_euclid(tau);
    if along <= span + 1e-4 {
        (a, b)
    } else {
        (b, a)
    }
}

/// Length of the containing arc from `start` to `end`, in `[0, τ)`.
pub fn containing_span(start: f32, end: f32) -> f32 {
    (end - start).rem_euclid(std::f32::consts::TAU)
}

/// Hold the middle, then swing out to each end of the containing arc.
pub fn sweep_angle(start: f32, end: f32, t: f32) -> f32 {
    let span = containing_span(start, end);
    let mid = wrap_pi(start + span * 0.5);
    if t < SWEEP_SETTLE_S {
        return mid;
    }
    let phase = (t - SWEEP_SETTLE_S) / SWEEP_PERIOD_S * std::f32::consts::TAU;
    wrap_pi(mid + (span * 0.5) * phase.sin())
}

/// 0 at the low pose, 1 at the far end of the talk trip, 0 again one period later.
fn talk_envelope(t: f32) -> f32 {
    let phase = t / TALK_WAVE_PERIOD_S * std::f32::consts::TAU;
    0.5 * (1.0 - phase.cos())
}

/// Arms from the low pose out to frame 20 and back. `t` 0 and the period are
/// frame 0. The peak, half a period later, is frame 20.
pub fn talk_wave(t: f32) -> f32 {
    talk_envelope(t) * TALK_WAVE_RAD
}

/// The same trip for the body, a couple of degrees, opposite sign to the arm
/// clock. The camera's remaining half turn mirrors X, so the body turns the
/// other way from `talk_wave` and the raised left hand stays on the viewer's right.
pub fn talk_yaw(t: f32) -> f32 {
    -talk_envelope(t) * TALK_YAW_RAD
}

/// What the gesture clock is counting. A change restarts it at the middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureMode {
    Rest,
    Talk,
    /// Highlight rect, as `to_bits` of u, v, w, h.
    Wave([u32; 4]),
}

#[derive(Clone, Copy, Debug)]
pub struct GestureClock {
    pub t: f32,
    mode: GestureMode,
}

impl Default for GestureClock {
    fn default() -> Self {
        Self { t: 0.0, mode: GestureMode::Rest }
    }
}

impl GestureClock {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Seconds since this mode began. A new mode returns 0 and does not
    /// consume `dt`, so the first sample is the middle of the new wave.
    pub fn sample(&mut self, mode: GestureMode, dt: f32) -> f32 {
        if self.mode != mode {
            self.mode = mode;
            self.t = 0.0;
        } else {
            self.t += dt.max(0.0);
        }
        self.t
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AimSpring {
    pub theta: f32,
    pub vel: f32,
}

impl Default for AimSpring {
    fn default() -> Self {
        Self { theta: 0.0, vel: 0.0 }
    }
}

impl AimSpring {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn step(&mut self, target: f32, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        if dt == 0.0 {
            return;
        }
        let w = AIM_OMEGA;
        let err = shortest_delta(self.theta, target);
        let acc = w * w * err - 2.0 * w * self.vel;
        self.vel += acc * dt;
        self.theta = wrap_pi(self.theta + self.vel * dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn yaw_matches_the_four_compass_points() {
        let pi = std::f32::consts::PI;
        assert!(near(aim_yaw(0.0), 0.0));
        assert!(near(aim_yaw(pi / 2.0), -pi / 2.0));
        assert!(near(aim_yaw(pi), 0.0));
        assert!(near(aim_yaw(-pi / 2.0), pi / 2.0));
        assert!(near(aim_yaw(3.0 * pi / 2.0), pi / 2.0));
    }

    #[test]
    fn point_time_lands_on_the_compass_frames() {
        let pi = std::f32::consts::PI;
        let dur = 15.0;
        assert!(near(point_time(0.0, dur), 0.0));
        assert!(near(point_time(pi / 2.0, dur), 3.75));
        assert!(near(point_time(pi, dur), 7.5));
        assert!(near(point_time(-pi / 2.0, dur), 11.25));
        assert!(near(point_time(3.0 * pi / 2.0, dur), 11.25));
        // One second per degree lands straight up at t = 180.
        assert!(near(point_time(pi, 360.0), 180.0));
    }

    #[test]
    fn screen_quadrants() {
        let pi = std::f32::consts::PI;
        let (fx, fy) = (100.0, 100.0);
        // y grows down. A larger x is the viewer's right.
        assert!(near(aim_theta(fx, fy, 100.0, 180.0), 0.0));
        assert!(near(aim_theta(fx, fy, 180.0, 100.0), pi / 2.0));
        assert!(near(aim_theta(fx, fy, 100.0, 20.0), pi));
        assert!(near(aim_theta(fx, fy, 20.0, 100.0), -pi / 2.0));
    }

    #[test]
    fn half_turn_tie_goes_through_the_low_pose() {
        let pi = std::f32::consts::PI;
        let right_to_left = shortest_delta(pi / 2.0, -pi / 2.0);
        assert!(right_to_left < 0.0, "{right_to_left}");
        assert!(near(right_to_left.abs(), pi));
        let left_to_right = shortest_delta(-pi / 2.0, pi / 2.0);
        assert!(left_to_right > 0.0, "{left_to_right}");
        assert!(near(left_to_right.abs(), pi));
    }

    #[test]
    fn side_to_side_is_damped() {
        let pi = std::f32::consts::PI;
        let mut spring = AimSpring { theta: pi / 2.0, vel: 0.0 };
        spring.step(-pi / 2.0, 1.0 / 30.0);
        assert!(spring.theta < pi / 2.0);
        assert!(spring.theta > 0.0, "one frame should not reach the low pose, got {}", spring.theta);
        let mut prev = spring.theta;
        for _ in 0..600 {
            spring.step(-pi / 2.0, 1.0 / 30.0);
            assert!(spring.theta <= prev + 1e-3, "turned back up: {prev} -> {}", spring.theta);
            assert!(spring.theta > -pi / 2.0 - 0.2, "overshot the left side: {}", spring.theta);
            prev = spring.theta;
        }
        assert!((spring.theta + pi / 2.0).abs() < 0.08, "settled at {}", spring.theta);
    }

    fn near_angle(a: f32, b: f32) -> bool {
        shortest_delta(a, b).abs() < 1e-3
    }

    #[test]
    fn overhead_box_starts_up_and_reaches_both_near_corners() {
        let pi = std::f32::consts::PI;
        // Figure at (100, 100). The near edge y=40 ties the two closest corners.
        let corners = [(40.0, 40.0), (160.0, 40.0), (40.0, 10.0), (160.0, 10.0)];
        let (start, end) = wave_endpoints(100.0, 100.0, corners);
        assert!(near_angle(start, 3.0 * pi / 4.0), "start {start}");
        assert!(near_angle(end, -3.0 * pi / 4.0), "end {end}");
        assert!(near_angle(sweep_angle(start, end, 0.0), pi));
        assert!(near_angle(sweep_angle(start, end, SWEEP_SETTLE_S - 0.01), pi));
        let out = sweep_angle(start, end, SWEEP_SETTLE_S + SWEEP_PERIOD_S * 0.25);
        let back = sweep_angle(start, end, SWEEP_SETTLE_S + SWEEP_PERIOD_S * 0.75);
        assert!(near_angle(out, -3.0 * pi / 4.0), "quarter {out}");
        assert!(near_angle(back, 3.0 * pi / 4.0), "three quarters {back}");
        let again = sweep_angle(start, end, SWEEP_SETTLE_S + SWEEP_PERIOD_S * 0.5);
        assert!(near_angle(again, pi));
    }

    #[test]
    fn side_box_waves_between_the_middle_corners() {
        let figure = (100.0_f32, 100.0_f32);
        let corners = [(200.0, 80.0), (280.0, 80.0), (200.0, 140.0), (280.0, 140.0)];
        let mut order = [0usize, 1, 2, 3];
        let dist = corners.map(|(x, y)| {
            let dx = x - figure.0;
            let dy = y - figure.1;
            dx * dx + dy * dy
        });
        order.sort_by(|&i, &j| dist[i].total_cmp(&dist[j]));
        let theta = |i: usize| aim_theta(figure.0, figure.1, corners[i].0, corners[i].1);
        let near = theta(order[0]);
        let mid_a = theta(order[1]);
        let mid_b = theta(order[2]);
        let far = theta(order[3]);
        let (start, end) = wave_endpoints(figure.0, figure.1, corners);
        assert!(near_angle(start, mid_a) || near_angle(start, mid_b), "start {start}");
        assert!(near_angle(end, mid_a) || near_angle(end, mid_b), "end {end}");
        assert!(!near_angle(start, end));
        let span = containing_span(start, end);
        let along = |t: f32| (t - start).rem_euclid(std::f32::consts::TAU);
        assert!(along(far) <= span + 1e-3, "farthest {far} outside {start}.. span {span}");
        assert!(along(near) > span + 1e-2, "nearest {near} fell inside the wave");
    }

    #[test]
    fn talk_wave_reaches_frame_twenty_and_the_body_barely_turns() {
        let frame = 20.0_f32.to_radians();
        let yaw = -2.0_f32.to_radians();
        assert!(near(talk_wave(0.0), 0.0));
        assert!(near(talk_yaw(0.0), 0.0));
        assert!(near(talk_wave(TALK_WAVE_PERIOD_S * 0.5), frame));
        assert!(near(talk_yaw(TALK_WAVE_PERIOD_S * 0.5), yaw));
        assert!(near(talk_wave(TALK_WAVE_PERIOD_S), 0.0));
        assert!(near(talk_yaw(TALK_WAVE_PERIOD_S), 0.0));
        assert!(talk_wave(0.4) > 0.0);
        assert!(talk_yaw(0.4) < 0.0);
        assert!(talk_wave(TALK_WAVE_PERIOD_S * 0.25) < frame);
        assert!(talk_yaw(TALK_WAVE_PERIOD_S * 0.5).abs() < talk_wave(TALK_WAVE_PERIOD_S * 0.5) * 0.2);
    }

    #[test]
    fn pointing_up_keeps_the_yaw_target_forward() {
        let pi = std::f32::consts::PI;
        assert!(near(aim_yaw(pi), 0.0));
        let mut pose = AimSpring::default();
        let mut yaw = AimSpring::default();
        for _ in 0..450 {
            pose.step(pi, 1.0 / 30.0);
            yaw.step(aim_yaw(pi), 1.0 / 30.0);
            assert!(yaw.theta.abs() < 0.02, "yaw left forward: {}", yaw.theta);
        }
        assert!(
            shortest_delta(pose.theta, pi).abs() < 0.05 || shortest_delta(pose.theta, -pi).abs() < 0.05,
            "pose settled at {}",
            pose.theta
        );
    }

    #[test]
    fn a_new_mark_restarts_the_gesture_clock() {
        let mut clock = GestureClock::default();
        assert_eq!(clock.sample(GestureMode::Talk, 0.5), 0.0);
        assert!(near(clock.sample(GestureMode::Talk, 0.5), 0.5));
        let wave = GestureMode::Wave([1, 2, 3, 4]);
        assert_eq!(clock.sample(wave, 0.5), 0.0);
        assert!(near(clock.sample(wave, 0.25), 0.25));
        assert_eq!(clock.sample(GestureMode::Rest, 0.1), 0.0);
    }
}
