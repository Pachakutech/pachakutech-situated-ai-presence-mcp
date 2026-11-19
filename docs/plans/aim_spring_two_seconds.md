# Aim spring settles in two seconds

Status: landed 18 November 2026 at `AIM_OMEGA` 2.0. Replaced the same day by `point_in_600ms.md`, then by `point_in_2400ms.md`. The frequency is 5/3, about 2400 milliseconds. `SWEEP_SETTLE_S` stays 5. The wave period stays 15 seconds.

18 November 2026. The lag before the arms arrive on a mark is the aim spring. Bring that settle from about five seconds to about two.

`AimSpring` in `daemon/src/avatar/aim.rs` is critically damped. `AIM_OMEGA` is 0.8 rad/s. The settle is about `4 / ω`, which is five seconds. The same spring carries the pose clock and the yaw. Both get quicker together.

## The change

`AIM_OMEGA` becomes 2.0. `4 / 2` is two seconds.

`SWEEP_SETTLE_S` stays 5. That hold is the pause in the middle of the wave before the arms travel to the corners. It is a separate timer from the spring. The wave period stays 15 seconds.

The comment on `AIM_OMEGA` should say a turn settles in about two seconds. The comment on `SWEEP_SETTLE_S`, and the two sentences in `docs/situated-presence.md` that say the hold matches a five-second spring (the natural-frequency sentence, and "She holds the middle of that arc for about five seconds, which is also how long the springs take to settle"), should say the spring settles in about two seconds and the hold in the middle stays five.

The sweep tests key off `SWEEP_SETTLE_S`. They stay. Nothing asserts the old frequency.

## Read with

- `daemon/src/avatar/aim.rs` `AIM_OMEGA`, `AimSpring::step`
- `daemon/src/avatar/mod.rs` `steer` — both springs step every frame, and only while she is talking
- `docs/situated-presence.md`
