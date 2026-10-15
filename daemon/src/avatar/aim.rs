//! Screen aim for the sitting figure.
//!
//! Theta is a clock on the screen: 0 below her, +π/2 to the viewer's right,
//! π above, −π/2 to the viewer's left. The same angle picks a frame of `Point`
//! and a root yaw. A critically damped spring carries theta, so a turn from
//! one side to the other settles instead of snapping. An exact tie between
//! the high pose and the low pose takes the low pose.

/// One breath. The 15 s clip is played through in this many seconds.
pub const BREATH_PERIOD_S: f32 = 4.0;

/// Natural frequency of the aim spring, in rad/s. Critically damped, so a
/// side-to-side turn settles in about a second.
pub const AIM_OMEGA: f32 = 4.0;

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

/// Screen y grows down. `atan2(dx, dy)`: 0 below, +π/2 viewer's right.
pub fn aim_theta(figure_x: f32, figure_y: f32, box_x: f32, box_y: f32) -> f32 {
    let dx = box_x - figure_x;
    let dy = box_y - figure_y;
    dx.atan2(dy)
}

pub fn box_center(u: f32, v: f32, w: f32, h: f32, screen_w: f32, screen_h: f32) -> (f32, f32) {
    ((u + w * 0.5) * screen_w, (v + h * 0.5) * screen_h)
}

/// `Point` time for this theta, wrapped into `[0, duration)`. Theta 0 is the
/// low pose. Theta π/2 is a quarter of the way through the clip.
pub fn point_time(theta: f32, duration: f32) -> f32 {
    if duration <= 0.0 {
        return 0.0;
    }
    (theta / std::f32::consts::TAU).rem_euclid(1.0) * duration
}

/// Yaw about +Y from the same theta. 0 above and below, +π/2 at the viewer's
/// right, −π/2 at the viewer's left.
pub fn aim_yaw(theta: f32) -> f32 {
    theta.sin().atan2(theta.cos().abs())
}

/// Point only while she is talking and a highlight is still on screen.
/// Otherwise the target is the arms-down pose.
pub fn gesture_target(talking: bool, aim: Option<f32>) -> f32 {
    match aim {
        Some(theta) if talking => theta,
        _ => 0.0,
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
        assert!(near(aim_yaw(pi / 2.0), pi / 2.0));
        assert!(near(aim_yaw(pi), 0.0));
        assert!(near(aim_yaw(-pi / 2.0), -pi / 2.0));
        assert!(near(aim_yaw(3.0 * pi / 2.0), -pi / 2.0));
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
    }

    #[test]
    fn screen_quadrants() {
        let pi = std::f32::consts::PI;
        let (fx, fy) = (100.0, 100.0);
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
        for _ in 0..120 {
            spring.step(-pi / 2.0, 1.0 / 30.0);
            assert!(spring.theta <= prev + 1e-3, "turned back up: {prev} -> {}", spring.theta);
            assert!(spring.theta > -pi / 2.0 - 0.2, "overshot the left side: {}", spring.theta);
            prev = spring.theta;
        }
        assert!((spring.theta + pi / 2.0).abs() < 0.08, "settled at {}", spring.theta);
    }

    #[test]
    fn gesture_requires_talking_and_a_mark() {
        assert_eq!(gesture_target(false, Some(1.0)), 0.0);
        assert_eq!(gesture_target(true, None), 0.0);
        assert_eq!(gesture_target(false, None), 0.0);
        assert!(near(gesture_target(true, Some(1.2)), 1.2));
    }
}
