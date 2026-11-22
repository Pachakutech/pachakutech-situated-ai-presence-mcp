# Starting a research tour

The client writes the claim, the quotes, and the narration. The machine maps the pages, stores the loci, and drives the glass. This card is the call. The daemon sources, the mapper, and the page HTML stay closed. Fetching a URL to pre-check a quote, and reading the server to learn the socket, are the costs this card removes.

## What you send

One citation per quote:

- `url`, a public `http` or `https` address. A local path is refused.
- `claim`, the sentence under test, at most 500 characters. Use the same claim on every citation. The project title is that claim, cut at 200 characters.
- `quote`, a short span that already occurs in the page text, at most 500 characters. One sentence or one verse, short enough to sit inside one mapped passage. A passage is at most 2000 characters, and a page keeps at most 2000 of them.
- `narration`, one or two sentences on why that quote bears on the claim. Intake cuts the line at 300 characters.

A second quote from the same page is another citation with the same URL.

## The call

1. The daemon socket is up before the call. When the user has asked for the tour and the socket is down, start the daemon with the voice environment already exported. The process copies that environment once, at start. When the socket is up, leave the process. Do not rebuild it, and do not restart it to pick up a newer tree.
2. `showCitations` takes the citations. Omit `mode` and `pace` for the narrated check. Those defaults are `explain` and `flow`. When the user will tap the register, pass `select` and `wait`. When that tool is not on the connected server, the same object goes to `node dist/cli.js show-citations`. The JSON is the argument, a file path, or stdin. A bare array is the citation list. `mode` and `pace` are fields on the object. Omit them for the narrated check.
3. That call creates the project, waits for the map, selects loci for ask `ak-1`, and starts the tour. The reply carries `projectId` and `state` `awaitingStart`. It does not open a window and it does not confirm.
4. `presence tour-start <projectId>` confirms on the command socket. The state becomes `active`.
5. Spawn a presence when none owns the body. `presence tour-drive <projectId> --presence <id>` opens the parked page, tints the quote, and speaks the narration. Leave that process running. The user stops it. Stopping it brings the marks down.

On this checkout the command is `node dist/cli.js`, for `daemon`, `show-citations`, `tour-start`, `spawn`, and `tour-drive`. The `presence` shim is not the binary those commands go through.

`show-citations` calls `showCitations` and stops when the reply is `awaitingStart`. It does not send `confirmTour`. Leave `createProject`, `queryProject`, `selectLoci`, and `startProjectTour` to that command.

One ask is enough when every source carries its own quote. `ak-1` is that ask.

A reply of `{ projectId, pending }` with no `state` means the map budget ended first. The budget is 20 seconds for each source that was pending on the first read. The command prints that object and stops. Poll `queryProject` until none are `pending`, then `selectLoci` and `startProjectTour`. The pages are already being fetched. A second fetch from the client does not help.

## What comes back on the glass

A quoted citation is one source and one locus. The glass paints the loci of the source that is open, so the page shows the quote the tour is on. The back arrow and the forward arrow step one source, which is the next quote. The first source has no back arrow. The last source has no forward arrow. The arrows follow the ends of the scroll.

Sending the quotes is the frontier citation walk. Leaving `quote` and `claim` off is a different walk: up to five passages per source that share a word with the ask. On a page that also carries commentary, the commentary outranks a short verse. With quotes in hand, send the quotes.

An exact hit stores the quote you sent and your narration. `matchKind` `exact` is that hit. A quote the page does not contain becomes one near locus, and the narration becomes "I couldn't find the quoted words. The closest passage is this one."

## While the tour is up

Yellow is skip, green is accept, red is deny. The user's hands write those rows. In `select` and `wait`, the narration is spoken once and the tour waits. A green or a red leaves for the next phrase after a short beat. Their scroll moves the mark and does not advance the phrase. Re-read `queryProject` when you need the register.

The driver is the process that watches the glass. A find line in its log is that process working. Leave it. Do not open the locator, the daemon log, or `queryTask` to supervise a find that is still in progress.

Export to BibTeX, CSV, or HTML is a later call. This start does not write those files.

Pass `again` only when the tour should, after the list, continue over skips still on the glass. Omit `again` and the tour ends with the list. The marks stay until the driver stops, the task is dismissed, or the tour window closes.

## Leave closed

The daemon sources, `src/locate`, the passage files, and a fresh download of the URL. The quote is a sentence you already hold. The map, the match, and the tint belong to the running tour.
