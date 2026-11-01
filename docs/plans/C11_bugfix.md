# C11-bugfix — the intake fetch reports what happened

Implementation brief. C11 is landed (`732ff8a`). This ticket follows the live trial of that build. It does not reorder C12 or C13.

The trial sent two copies of one address-bar URL. Both pages were already open in the browser. Intake stored both as `failed` / `unreachable`. The daemon log has no line for the fetch. A repeat of the checker's own curl flags shows why: Cloudflare answers the default client with `HTTP/2 403` and `cf-mitigated: challenge`, and `fetch_url` then throws that status away.

Read first:

- `daemon/src/intake.rs` (`LiveProbe::fetch`, `LiveProbe::fetch_body`, `parse_curl_headers`, `fetch_url`)
- `daemon/src/project.rs` (`prepared_from`, `source_records`, `route_project_with` calling `LiveProbe` on the socket path)
- `daemon/src/adapt.rs` (`fetch_final`, which maps with `fetch_body` and also collapses a non-2xx page to `unreachable`)
- `docs/plans/C8_C13_research_tours.md` (L7 and L8; the C8 prose that says the fetcher records the status)

## Goal

A failed URL check stores a short class the client can read back on `queryProject`, and the daemon log gains one line for that failure. Both curls send one browser User-Agent. A 403 remains a failure. A host that never answers stays `unreachable`.

## What the trial showed

Daemon: the release binary staged this session, started 1 Nov 2026 09:55, log `/run/user/1000/pachakutech/presence-daemon.log`. Projects root `~/.local/share/pachakutech/projects` (`XDG_DATA_HOME` is `~/.local/share`; `PRESENCE_PROJECTS_DIR` was unset).

One `createProject` on `command.sock`, proposal `c11-try-create`, title `The Waste Land`, ask `ak-1` (`question`: "Where does Eliot show the inadequacy of spoken and written communication?"), no claim and no quote. Both sources were the same URL, copied from the address bar of a page that was open:

`https://www.poetryfoundation.org/poems/47311/the-waste-land`

The reply was `ok` in about 360 ms, `draft: false`, `projectId` `pj-d147167013144df6`. The store still holds that project. `project.json` (868 bytes, mtime 09:55, `createdAt` 1793555753610, `createdBy` `T1`):

| source | status | reason |
|---|---|---|
| `sr-1` web, that URL | `failed` | `unreachable` |
| `sr-2` web, the same URL again | `failed` | `unreachable` |

`loci.jsonl`, `events.jsonl`, and `tasks.jsonl` are empty. `selectLoci` for `ak-1` returned `locusIds: []`. `startProjectTour` in `approve` mode returned `sources: []` and `state: "active"`, with no `taskId`. Failed sources are omitted from the tour list, so there was no item to accept or deny. Duplicate URLs are stored as two sources. That is current intake behavior; this ticket does not collapse them.

## What the log contains

The log does not mention `pj-d147167013144df6`, `poetryfoundation`, `curl`, or the word `unreachable`. Socket success is quiet. The only line that names the trial is the spoken report on presence `p-peg4a7` (log line 169244).

A search for `403` hits appearance rectangles such as `webcam 403,159`. Those are pixel coordinates. They are not the HTTP status.

`LiveProbe` never calls `eprintln!`. `Command::output` captures stderr and the code never reads `output.stderr`, so a curl diagnostic would be dropped even if curl had printed one. On this failure curl printed nothing.

## What the checker did

`LiveProbe::fetch` (`intake.rs`) runs:

```text
curl -sS -D - -o /dev/null --max-redirs 0 --max-filesize 52428800
     --max-time 20 --proto =http,https --url <url>
```

No `-A`. No `-f`. `fetch_body` uses the same flags and writes the header and body to temp files `presence-curl-h-*` and `presence-curl-b-*`. Mapping (`adapt.rs` `fetch_final`) calls `fetch_body`.

`parse_curl_headers` keeps the last `HTTP/` line and reads the status as the second whitespace token. `HTTP/2 403` parses as status 403. The header walk keeps `content-length` and `location` only. It does not keep `cf-mitigated`.

`fetch_url` then:

- `FetchReply::Unreachable`, a fourth redirect hop, or any other leftover → `Ok(Some("unreachable"))`
- `content-length` over 50 MB → `oversize`
- 300–399 → follow, at most three hops, then the same URL rules
- 200–299 → `Ok(None)`, source stays `pending`
- every other status, including 403 → `Ok(Some("unreachable"))`

`Prepared.reason` is `Option<&'static str>`, so the stored word has to be a compile-time constant today. `Source.reason` on the project record is already `Option<String>`. The static lifetime is what forces one shared word. `fetch_url`'s return type is the same `&'static str`.

A private or disallowed URL is `CheckError::UrlNotAllowed` and fails the whole proposal. That path did not run here. Per-source failure still keeps the next record (L7).

`createProject` runs `LiveProbe` on the socket path (`project.rs`, `route_project_with`). Two URLs can each wait up to 20 s on that thread. This trial returned in 360 ms, so the stall is not what failed the page. This ticket leaves the call where it is.

## Reproduction, 1 Nov 2026

curl 8.21.0, OpenSSL/3.6.3, the flags above, the poem URL. Body discarded.

| Client | Status line | Other headers | stderr |
|---|---|---|---|
| curl's own User-Agent | `HTTP/2 403` | `cf-mitigated: challenge`, `server: cloudflare`, `content-type: text/html; charset=UTF-8` | empty |
| `User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36` | `HTTP/2 200` | `server: cloudflare`, `content-type: text/html;charset=utf-8`, `content-length: 928886` | empty |

The checker received a real response. It stored `unreachable` because 403 is outside 200–299 and 300–399. A browser User-Agent is enough for this page with this curl. The same omission in `fetch_body` means a later map of a pending copy of this URL would meet the same 403 and collapse it again inside `fetch_final`.

C8's brief says the fetcher records the status. The landed code parses the status and then discards it.

L7's missing URL is not this case. `probe_ok(&[])` returns `FetchReply::Unreachable` for a URL absent from the script, and the test expects the reason `unreachable`. That string stays for a probe that never got a status.

## Decisions that bind

1. **One argument list for both curls.** `LiveProbe::fetch` and `LiveProbe::fetch_body` build argv from one function. The list keeps `-sS`, `--max-redirs 0`, `--max-filesize 52428800`, `--max-time 20`, and `--proto =http,https`. It adds `-A` with one fixed browser User-Agent, the Chrome string in the table above, as a named constant. It does not add `-f`. `-f` would hide the status and bring back an empty failure.

2. **The reason names the class.** Change `Prepared.reason` and `fetch_url`'s success value from `&'static str` to an owned `String`. `Source.reason` stays `Option<String>`. The stored value is one of:

| Condition | Stored reason |
|---|---|
| 200–299 | none; source stays `pending` |
| 300–399 | unchanged redirect walk |
| any other HTTP status | `http-<status>` (example `http-403`) |
| that status, and a header `cf-mitigated` whose value contains `challenge`, ASCII case-insensitive | `http-<status> challenge` (example `http-403 challenge`) |
| `content-length` or curl exit 63 over 50 MB | `oversize` |
| curl exit 6 | `dns` |
| curl exit 28 | `timeout` |
| `Command` failed to spawn curl | `spawn` |
| `FetchReply` miss, curl exit 7, status 0 with no class above, redirect past hop 3 | `unreachable` |

`parse_curl_headers` and `parse_curl_body_headers` report the challenge flag. `FetchReply::Response` carries it (a `bool` is enough). `fetch_final` uses the same table, so a mapping failure stores the same class on the source instead of a second copy of the word `unreachable`.

The reason is that short class only. It is not the body, not a header dump, and not stderr. Cap it so a surprise status still fits in a small string (`http-` plus the code, plus ` challenge` when the flag is set).

3. **One log line on a failure.** `eprintln!` with the prefix `[intake]`, the same style as `[project]` and `[socket]`. The line carries the URL, the class, the HTTP status when there is one, and the curl exit code when there is one. Source ids `sr-N` are assigned later in `source_records`, so this line names the URL. Success (2xx, and a redirect that is still being followed) logs nothing. Do not log the response body. Stderr stays out of the project file. A clipped stderr sentence may be appended on the log line only, at most 120 characters, and only when the class is `dns`, `timeout`, `spawn`, or `unreachable`. The 403 in this trial had empty stderr; the status line is what was missing.

4. **The client reads the class from `queryProject`.** `source.reason` is already on that reply. `createProject` still returns `draft` and `projectId`. No new socket kind and no new field.

5. **A 403 is still `failed`.** The User-Agent is there so a page that answers a browser can be stored as `pending` and mapped. A challenge that remains after that header is `http-403 challenge`, status `failed`, and the tour still omits it. Do not treat 403 as success by ignoring the status.

6. **L7 and L8 keep `unreachable` for a true miss.** `probe_ok` returning `FetchReply::Unreachable` still stores that word. The new classes are additional arms. `url_not_allowed` still rejects the whole proposal.

## Tests

| ID | What this ticket has to show |
|---|---|
| F1 | A scripted response, status 403, challenge false, stores `failed` / `http-403`. The next source is still stored |
| F2 | The same response with challenge true stores `http-403 challenge` |
| F3 | `FetchReply::Unreachable` still stores `unreachable`. L7 and L8 pass unchanged for that miss |
| F4 | Status 200 still stores `pending` and no reason |
| F5 | The shared argv builder includes `-A` and the User-Agent constant, includes `--proto` `=http,https`, and does not include `-f`. `fetch` and `fetch_body` both use it |
| F6 | `fetch_final` on a `fetch_body` page of status 403 fails that source with `http-403`, the same class intake stores |
| F7 | The log-line function for a 403 challenge contains `[intake]`, the URL, `http-403 challenge`, and the status. It does not contain the body |

F5 and F7 are pure. They do not hit the network. F1–F4 and F6 use the scripted probe, the same way L7 does.

## Out of scope

| Left alone | What |
|---|---|
| C12, C13 | Exports and the MCP surface. Still paused |
| B1b, C11b | Retrieval failover and the pause-time highlight. Separate briefs, not written here |
| The tour wiring | `AllowOpener` records and does not launch Chromium. The overlay does not draw yes/no. `runTour` has no production caller. This trial never reached those, because both sources failed first |
| Duplicate URLs | `sr-1` and `sr-2` are the same string. Intake keeps both |
| Where `LiveProbe` runs | It stays on the socket path. The 20 s `--max-time` stall is a later ticket |
| Redirect policy | Three hops, then `unreachable`. A redirect into a refused host is still `url_not_allowed` |
| The viseme failure | `speech::tests::avatar_speak_and_rest_commands_route_to_speaker` stays as it is |
| Warnings | No `cargo fix` |
| This project | Do not re-fetch `pj-d147167013144df6` from a side door, and do not author passages for it |

No new crate. No new socket kind. No edit to the gate's allow-list.

## Done when

F1 through F7 pass, and L7 still expects `unreachable` for the scripted miss. Commit title: `daemon: report the HTTP status of a failed fetch (C11-bugfix)`.
