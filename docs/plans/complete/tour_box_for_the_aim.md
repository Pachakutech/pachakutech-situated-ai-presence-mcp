# The tour box for the aim

Status: landed 18 November 2026. One aim key on the presence, written by the driver. `highlightRegion` takes `aim`. `visible_interest` returns that mark's box while it is visible. The spring later moved to 5/3, about 2400 milliseconds, in `point_in_2400ms.md`. The 1600 ms step was not taken. Heat later stamps the aim rectangle only, in `tour_page_marks.md`.

18 November 2026. During a research tour the bar is on screen and she is talking, and the arms do the talking lift. The aim never receives the box.

Project `pj-4cc62de396d8b417`, presence `p-khgyge`. The allyl line was up as `lc-4ab515a788f6f2fe` while she spoke. She sat at `480,800`. The line started at y 638, about 125 degrees up from that seat. The talking path raises an arm 20 degrees.

## The miss

`highlightRegion` stores the mark under the phrase id when the tour sends one, and under the presence id when it does not (`declare_interest` in `daemon/src/registry.rs`). The glass and the highlight log keep that key. `lc-4ab515a788f6f2fe` is the key for this bar.

The aim asks `visible_interest` for the presence that owns the body (`daemon/src/overlay.rs`, `bound_presence`). That lookup matches the key exactly (`daemon/src/socket.rs`). `p-khgyge` is not the key, so the aim is handed no box. Talking with no box is `talk_wave`, 20 degrees, and about 2 degrees of yaw.

## The aim box

Each presence has one aim key. The avatar reads that key's visible box. It does not walk the marks, and it does not pick the latest paint.

`highlightRegion` gains `aim`, default false.

- A declare with no `phraseId` is the aim. The key stays the presence id. A teaching highlight keeps pointing at that box.
- A declare with `phraseId` and `aim` sets that presence's aim key to the phrase. The mark is still stored under the phrase id.
- A later declare of that same phrase updates the rectangle the aim reads. Scroll and settle already re-declare the phrase. They do not send `aim` again.
- A declare of another phrase, with `aim` false, paints that mark and leaves the aim key alone.
- A declare with `aim` on a different phrase replaces the key. That is how the sidecar moves the point, and how the tour moves to the next locus.

`visible_interest(presence_id)` returns the aim mark's box while that mark is visible. A faded mark is not returned. The talking lift stays when the sample is empty. Retiring the aim key clears it. Concealing that phrase is the tour's `concealHighlight` with the phrase id, which already retires the mark.

The glass, the conceal path, the highlight log, and the heat stamp keep the phrase key. Heat still stamps every visible mark. This does not change the book of marks the glass draws.

The driver in `src/locate/driveTourLive.ts` is the writer. `highlight` of the current item sends `aim` with that phrase. `shiftAll` and `watchGlass` keep painting every held phrase. Only the phrase that was named the aim moves the arms. A second bar staying up after the list does not take the point.

The sidecar uses the same field. It paints several phrases and sends `aim` on the one box it chose. Each presence has its own key, so a later avatar samples its own presence.

A scan that takes the latest declaration owned by the presence was set aside. After the list, several bars stay up, and a scroll paints every bar that moved. The last paint would become the aim. The sidecar has that same shape, and the writer is the one who chooses.

## Leave

`AIM_OMEGA` was left at 6.5 when this ticket landed. A quarter turn from rest had under a tenth left at 600 milliseconds. That arrival had not been seen on a tour. The same day, before that look, `point_in_2400ms.md` set the frequency to 5/3. The sentences in `docs/situated-presence.md` name 2400 milliseconds. The test is `a_quarter_turn_arrives_in_2400_milliseconds`.

`SWEEP_SETTLE_S` stays 5. The wave period stays 15 seconds. `GestureMode::Wave` is the bits of the rectangle, so a scroll that changes the aim box starts the five-second hold again. The springs do not reset. They ease toward the new middle of the arc. Leave that clock.

The spring and the 33 ms sample stay in the daemon. The overlay thread does not ask Node which box to use.

## Tests

- A tour-style declare, presence `p-a`, phrase `lc-1`, `aim` set, leaves a visible box. `visible_interest("p-a")` returns that box. `visible_interest("p-b")` returns none. The highlight key remains `lc-1`.
- A second declare, presence `p-a`, phrase `lc-2`, `aim` omitted, leaves `visible_interest("p-a")` on the `lc-1` box. `lc-2` is in the book.
- A later declare of `lc-1` with a new rectangle updates the box `visible_interest("p-a")` returns.
- Concealing `lc-1` makes `visible_interest("p-a")` return none.
- A declare with no phrase id still makes `visible_interest("p-a")` return that box.
- The sweep tests stay. The arrival test is `a_quarter_turn_arrives_in_2400_milliseconds`.

Landed in `daemon/src/registry.rs`: `the_named_phrase_is_the_aim`, `another_phrase_leaves_the_aim`, `a_later_declare_moves_the_aim_box`, `concealing_the_aim_phrase_clears_it`, `a_declare_without_a_phrase_id_is_the_aim`, `a_faded_aim_is_not_returned`, and `closing_the_connection_clears_its_aim`. The wire default is `highlight_aim_defaults_off_and_parses_on`. The driver check is in `test/driveTour.test.ts`.

## Read with

- `daemon/src/highlight.rs` `Interest`, `visible`
- `daemon/src/registry.rs` `declare_interest`, `retire_interest`
- `daemon/src/protocol.rs` `HighlightRegion`
- `daemon/src/socket.rs` `visible_interest`
- `daemon/src/overlay.rs` the `steer` call
- `daemon/src/avatar/mod.rs` `steer` — a box while talking is the wave, and no box while talking is the 20 degree lift
- `src/locate/driveTourLive.ts` `paint`, `highlight`, `shiftAll`, `watchGlass`
- `docs/plans/point_in_2400ms.md` — the frequency after this box landed
