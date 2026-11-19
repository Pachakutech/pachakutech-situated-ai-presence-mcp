# Point arrives in 600 milliseconds

Status: landed 18 November 2026. `AIM_OMEGA` is 6.5. A quarter turn from rest has under a tenth left after 600 milliseconds. The test is `a_quarter_turn_arrives_in_600_milliseconds`. `SWEEP_SETTLE_S` stays 5. The wave period stays 15 seconds. `tour_box_for_the_aim.md` hands the aim a box and leaves this frequency. The 1600 ms spring is deferred.

18 November 2026. On the inductive sentence the bar and the avatar sat level, and the arms still took a few seconds to leave the lap, and then only a little. The lag to the commanded angle is the aim spring. Bring that arrival under a second, about 600 milliseconds.

Project `pj-27399da9c90dea2b`, presence `p-2iyoxs`. The inductive bar stayed `900x37` at `982,816`, scroll 81, every settle `track`. She stayed at `480,800` the whole time it was up. The cue was 7.16 seconds and then 4.09. `AIM_OMEGA` is 2.0, the two-second settle.

From that seat the held angle is about 98 degrees, a sideways point just above the horizon, and the wave around it is about 11 degrees. At frequency 2 a quarter turn is about 35 percent done at 600 milliseconds and about 60 percent at one second, which is still a small lift off the lap.

## The change

`AIM_OMEGA` becomes 6.5. The step response of `AimSpring` is `e^{-ωt}(1+ωt)`. At 6.5 a quarter turn is about 90 percent done at 600 milliseconds and past 98 percent at one second. The same spring carries the pose clock and the yaw, so both arrive together.

`SWEEP_SETTLE_S` stays 5. That hold is the pause in the middle before the arms travel the arc. The wave period stays 15 seconds.

The comment on `AIM_OMEGA` should say a turn arrives in about 600 milliseconds. In `docs/situated-presence.md`, the natural-frequency sentence and the line "The springs settle in about two seconds" should say about 600 milliseconds. The five-second hold stays named as five.

## Test

In `daemon/src/avatar/aim.rs`, from rest toward `π/2`, after 0.6 seconds of `AimSpring::step` the remaining error is under 10 percent of the step. The sweep tests stay. They key off `SWEEP_SETTLE_S`.

## Read with

- `daemon/src/avatar/aim.rs` `AIM_OMEGA`, `AimSpring::step`
- `daemon/src/avatar/mod.rs` `steer`
- `docs/plans/aim_spring_two_seconds.md` — landed, and this replaces the two-second frequency
- `docs/situated-presence.md`
