# Point arrives in 2400 milliseconds

Status: landed 18 November 2026. `AIM_OMEGA` is 5/3. `4 / ω` is 2.4 seconds. A quarter turn from rest has under a tenth left after 2400 milliseconds. The test is `a_quarter_turn_arrives_in_2400_milliseconds`. `SWEEP_SETTLE_S` stays 5. The wave period stays 15 seconds.

18 November 2026. The tour aim has a box, and that arrival has not been seen yet. The 600 ms spring stays fast for the first look at the allyl line, and a 1600 ms step was written down as the later change if 600 still felt late. The arrival to use before that look is 2400 milliseconds.

## The change

`AIM_OMEGA` becomes 5/3. The step response of `AimSpring` is `e^{-ωt}(1+ωt)`. At 5/3 a quarter turn is about 90 percent done at 2400 milliseconds. The same spring carries the pose clock and the yaw.

`SWEEP_SETTLE_S` stays 5. The wave period stays 15 seconds. A new rectangle still restarts the wave hold. The springs do not reset.

The comment on `AIM_OMEGA` says a turn arrives in about 2400 milliseconds. In `docs/situated-presence.md`, the natural-frequency sentence and the springs line say about 2400 milliseconds. The five-second hold stays named as five.

## Test

In `daemon/src/avatar/aim.rs`, from rest toward `π/2`, after 2.4 seconds of `AimSpring::step` the remaining error is under 10 percent of the step. The sweep tests stay. They key off `SWEEP_SETTLE_S`. `side_to_side_is_damped` still steps 600 frames at 1/30 of a second. That count is the side-to-side run, not the arrival time.

## Read with

- `daemon/src/avatar/aim.rs` `AIM_OMEGA`, `AimSpring::step`
- `docs/plans/point_in_600ms.md` — landed at 6.5, and this replaces that frequency
- `docs/plans/tour_box_for_the_aim.md` — the box is landed, and this does not move the aim key
- `docs/situated-presence.md`
