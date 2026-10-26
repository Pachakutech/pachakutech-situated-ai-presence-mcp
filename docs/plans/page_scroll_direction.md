## Plan: The page walks to the bottom before it may climb

Record of the Waste Land reading on 2026-10-25. The code is unchanged.

### What the run did

The first cue was the hermit-thrush. The fragment put the stanza on the glass, the box was accepted, and the method on that success was `text-fragment+ocr`. The opening call does not pass a step ratio, so any walk it takes is about seventy percent of the window. On this pane that step was 756 pixels. The follower, for every later cue, uses a quarter of the window, 270 pixels, and asks for a text fragment first.

The follower log after that first box, in `/run/user/1000/pachakutech/cue-follow.log`:

- The third person, the stanza under the thrush. Fragment, then about thirty steps down, one jump to scroll `10916.4140625`, more steps down, then steps up. The attempt ended at the 90 second limit. Miss.
- The cock, the thunder, and the surrender lines. Fragment, one 80 pixel nudge, box shown.
- Dayadhvam. Fragment, about twenty-seven steps down, then steps up, until the limit. Miss.
- The boat. The same down-then-up, until the limit. Miss.
- The shore. The fragment landed and the crop check failed. Miss. That one did not take the long walk.
- The closing lines. Fragment, one nudge, box shown.

What you saw follows from that. A cue the reader already has in view is a nudge. A cue it does not accept is a march to the bottom of the Wikisource article, one turn, and a march back up. The article continues through Eliot's notes, so the bottom is far below Part V. The next cue starts the march again.

### Why a phrase above the window is reached only after the bottom

The search has one direction until the picture stops changing. With no stored scroll for this phrase, that direction is down. A positive step moves the document down, and the words rise. The follower's step is a quarter of the window. The opening call's step is most of the window.

The fragment is supposed to leave the phrase on the glass. The read that follows accepts a `start ... end` query when both ends are in that one read. When they are not, the phrase counts as absent, and the next action is a downward step. The picture stops changing when the scroller can go no further. On this page that place is the bottom of the article, notes included. The search then turns around once and takes the same sized steps upward. The turn is that one reversal. There is no shorter climb, and there is no second reversal.

The 90 second limit covers the whole attempt. The trip down spends most of it. The climb is still in progress when the attempt is given up. The stanza the downward steps already passed is still above the window at the end.

The third person is the stanza just under the thrush. The notes file's Read section contains `Who is the third who walks always beside you?` in several views taken during the march. A cue is filed only when the whole span matches, that opening and `on the other side of you` together. The opening was on the glass. The span was not accepted, so the search did not stop. By then the window was below the stanza. The way back up was the bottom of the article.

### Why the notes did not steer it

A stored scroll is consulted for the phrase that was stored, on an attempt that loads the file and can see that number. A miss is not tried again. The attempt that is still looking is the one that would have to write the number.

The file from this reading, `~/.local/share/pachakutech/pages/en-wikisource-org-wiki-the-waste-land-eliot-1922-29f608.md`, ended with these cues:

- Cock, thunder, and the surrender lines, each at `11776`. Those three were placed by the fragment and one nudge. The stored number is what a later attempt would open. It is not what placed them this time.
- Dayadhvam and the shore at `12290.20703125`. The closing lines and the boat at `12369.6552734375`. Those sit at the bottom. They were written when the long walk reached that part of the page. The attempts had already spent the limit on the way back up.

The third-person attempt did jump once, to `10916.4140625`, after the downward steps had started. That number was a scroll this same attempt had just remembered, from a frame in which the span was judged present. The jump happens once. The steps after it still default downward. The log shows further downward steps after the jump, then the turn, then the limit.

The loader does not read a fractional scroll back. A cue line is kept when the number is digits only. A pass heading is kept when the heading is digits only. The writer stored Chrome's `scrollTop`, and on this page that value is often a fraction. The next attempt loads the file and drops every fractional cue. The save after that writes the poorer copy over the file. The `10916.4140625` sighting is gone. The fractional lines still in the file were written by the last attempt. A later load drops those too.

A phrase with no integer on file has no direction. The direction stays down. A phrase stored as a fraction is absent on the next load. The integer entries are three phrases the fragment had already found. That is the whole effect of the notes on this reading.

### What has to change

1. A search that has passed the stanza has to be able to step up before the bottom of the article. One reversal at the stuck picture is a trip the length of the page, and the limit expires on the way back.
2. A scroll remembered for this phrase has to be the place the search looks, and the steps after that look have to stay with the phrase. A downward step after the jump leaves it.
3. The number in the file has to be the number the loader reads. Fractional scroll positions are written and then discarded, and the next save erases them.
4. A span whose end is still below the fold is stored as prose in Read and is not stored as a cue. The third-person opening is in the file in that condition, with no scroll a later attempt can open.
