# B3 — doctor, config, and placement

Implementation brief. `presence doctor` already runs. It checks Linux x64, Node, the bundled daemon, Wayland, the daemon socket, and the render node, then prints warn lines and returns. Warns do not change the exit status, and they do not stop `presence mcp`. This ticket adds the local-model report to that command. It does not start a server, and it does not start the daemon.

Protocol rows named B3 are the landed parse checks. This file is the ticket. Its tests are numbered DR.

Read first:

- `docs/plans/local_multimodal_reflex.md` sections 5, 6, 7, and 8 (one 4-bit job, CPU or iGPU behind the disc submits, doctor reports reachability, Iris Xe numbers are a record)
- `docs/plans/B1_sidecar.md` (the client that will read this config; it does not spawn the server; one completion, 3 s timeout)
- `docs/plans/B2_crops.md` (the crop long side is 384 to 1024; vision is off unless `PRESENCE_LFM3_VISION` is set)
- `docs/plans/voice_command.md` (`PRESENCE_VOICE_ANSWERS_APPROVE` is not a behavior switch)
- `docs/plans/command_layer_protocol.md` H6 and H9, and K2
- `src/cli.ts` (`doctor`) and `src/daemonControl.ts` (`socketReachable`, 400 ms)

## Goal

`presence doctor` also says whether the model endpoint and `sidecar.sock` are there, and it prints the placement rule for this machine class. The overlay is not part of the command. A missing model prints a warn and the process exits 0.

## What already stands

Do not replace these checks. Append the model report after them.

- The command is `presence doctor`. The labels include `Linux x64`, `Node >= 18`, `bundled presence-daemon`, `WAYLAND_DISPLAY set`, `daemon socket reachable`, and `/dev/dri/renderD128 accessible`.
- A warn line is `warn`. A passing line is `ok`. The closing sentence says warn items do not block `presence mcp`.
- `socketReachable` connects, then destroys the socket, and does not write a proposal. The timeout is 400 ms.
- The daemon does not read `PRESENCE_LFM3_ENDPOINT`. Nothing connects to `sidecar.sock` yet. B1 and B2 are specified and not built.

## Out of scope

| Left to | What |
|---|---|
| B1 | The completion, the mapper, the 3 s job timeout. This ticket exports the config that client will import. |
| B2 | Cutting a crop. This ticket names the ceiling the cut will read. |
| B4 | An in-daemon actor, a fine-tune, a GPU layer inside the daemon. |
| `sidecar_representation.md` | The collection walk that would call the endpoint this ticket reports. This ticket stays the report. |
| `voice_command.md` | Opening a microphone, or letting a transcript approve. |
| The operator | Starting `llama-server` or Ollama. This command does not spawn either. |
| K2's number | A live frame-time measurement. The procedure is below. The suite does not fail on an unmeasured budget. |
| The live daemon | Not restarted. No Rust file changes, so the viseme failure is untouched. |

No new socket kind. No MCP tool. No shader edit. No edit to `gate.rs`.

## Decisions that bind

1. **The report is a function the existing doctor calls.** `sidecarReport(env, probes)` returns lines. `doctor()` prints them after the current checks and keeps the closing sentence. Probe errors become warn lines. The function does not throw to the command. The process still exits 0.

2. **Two different "down" states.** `sidecar.sock` is the daemon's T3 listener (`PRESENCE_SIDECAR_SOCKET`, else the sibling of the presence socket, the same rule as `sidecarSocketPath` in the B1 brief). The endpoint is `PRESENCE_LFM3_ENDPOINT`, the base URL of `POST /v1/chat/completions`. Either may be absent while the other is up. The socket probe is `socketReachable` and sends no bytes. The endpoint probe is `GET {base}/v1/models` with a 1 s timeout. Unset endpoint: do not call the network.

3. **Config lives in `src/sidecar/config.ts`.** B1's client and B2's ceiling read this module rather than parsing the env again.

| Name | Default | Rule |
|---|---|---|
| `PRESENCE_LFM3_ENDPOINT` | unset | Empty is unset. No completion is attempted by the doctor. |
| `PRESENCE_LFM3_MODEL` | unset | When the probe returns a list and this id is absent, the endpoint line stays ok and a warn says the id is not in the list. |
| `PRESENCE_LFM3_JOBS` | 1 | Any other value warns and the effective value stays 1. There is no second slot. |
| `PRESENCE_CROP_LONG_MAX` | 1024 | An integer from 384 to 1024 lowers the ceiling. Anything else warns and the ceiling stays 1024. The floor stays 384. B2's pad and downsample use this ceiling. |
| `PRESENCE_LFM3_VISION` | off | `1` is on. Any other value is off. |
| `PRESENCE_VOICE_ANSWERS_APPROVE` | off | The doctor may print it. No daemon file reads it. `1` does not enable a path. |

4. **Lines, after the existing checks.** Stable labels:

```text
model sidecar
  ok|warn  sidecar socket
  ok|warn  endpoint
  ok|warn  model
  ok       jobs  — 1
  ok|warn  crop long side  — 384..<ceiling>
  ok|warn  vision
  ok|warn  voice answers approve  — off (report only)
  ok       placement  — cpu, one 4-bit job, -ngl 0, off the overlay thread
  ok       frame budget  — unmeasured
```

`endpoint` is warn when unset or unreachable, ok when `/v1/models` returns. `sidecar socket` is warn when the probe is false. `voice answers approve` is warn when the env is `1`, and the note still says the daemon ignores it. `frame budget` stays the unmeasured line. This ticket does not write a measured number into the source.

5. **Placement for this machine class.** Intel Iris Xe shares one Vulkan device with the daemon when the daemon is up. The model process is separate. The operator starts it with GPU layers off (`llama-server -ngl 0`, or the Ollama equivalent). One 4-bit GGUF, on the order of 1 GB. A second model is not part of the setup. Completions stay in that process, one at a time, and a late one returns B1's default judgement. The daemon's expand pass and the disc submits do not wait on it. The screen-text thread keeps its own capture. The doctor does not set a device, a priority, or an environment for a child, because it has no child.

6. **H9's frame hitch is avoided by where the job runs, not by a timer in the daemon.** Inference is not on the overlay thread. This ticket adds no skip-tick path in `overlay.rs`. B1's timeout is the skip. The doctor only states that.

7. **K2 is a manual note, not a test.** When a sidecar is actually up, the operator runs a 12-item flow tour with one completion in flight and writes the observed frame time into the commit message. Until that happens, the line stays `unmeasured`. Do not invent a frame rate in the source to satisfy the row.

8. **The command does not spawn, and it does not speak the daemon's protocol.** `sidecarReport` does not contain `spawn`, `llama-server`, `sendProposal`, or `shutdown`. A socket probe that connects and closes is the existing helper. No proposal JSON is written.

9. **`npm run build` runs** because `src/cli.ts` changes. Restore tracked `dist`. Do not commit it. No Rust edit, so the daemon suite is not a gate for this ticket.

## Measurement, when someone runs it

On the Iris Xe machine, with the daemon already up and `llama-server -ngl 0` already running: start one judge completion, run a 12-item flow tour, and record the overlay's frame time from the daemon log beside the idle time. One pair of numbers in the commit message is the whole of K2 for this ticket. A regression is a later comparison. The suite stays green without that pair.

## Acceptance

| ID | What B3 has to show |
|---|---|
| DR1 | Unset endpoint: no fetch, the endpoint line is warn, and the report resolves |
| DR2 | A fetch that rejects: the endpoint line is warn and says unreachable. The report resolves |
| DR3 | A model list that contains `PRESENCE_LFM3_MODEL`: the endpoint line is ok and the model line names that id |
| DR4 | A model list that omits the configured id: the endpoint line stays ok and the model line warns |
| DR5 | `PRESENCE_LFM3_JOBS=4` prints jobs as 1 |
| DR6 | Crop ceiling 512 is reported. `100` and `2000` warn and the reported ceiling stays 1024. The floor in the line stays 384 |
| DR7 | Vision unset is off. `PRESENCE_LFM3_VISION=1` is on |
| DR8 | `PRESENCE_VOICE_ANSWERS_APPROVE=1` warns and says the daemon ignores it. `daemon/src` does not contain that name |
| DR9 | The report source does not contain `spawn`, `llama-server`, `sendProposal`, or `shutdown`. The existing doctor still contains `Linux x64` and the sentence that warns do not block `presence mcp` |
| DR10 | A false socket probe reports the sidecar socket as warn. A true probe reports ok. The probe writes no bytes |

H6 for this ticket is DR1, DR2, and DR10, and the process exit stays 0 because `doctor()` does not set a failing status. H9's hitch rule is decision 6. K2 is the measurement section.

## Done when

DR1 through DR10 pass under `node --test test/doctor.test.ts` from the checkout, and the rest of `node --test test/*.test.ts` stays green. `npm run build` passes and `dist` is restored. Commit title: `command: sidecar reachability on presence doctor (B3)`. The live daemon is not restarted. Nothing is pushed.

## Files

- `src/sidecar/config.ts` — the env table above
- `src/sidecar/doctor.ts` — `sidecarReport`
- `src/cli.ts` — call it from the existing `doctor()`
- `test/doctor.test.ts`

No daemon file. No skill paragraph. No model binary.
