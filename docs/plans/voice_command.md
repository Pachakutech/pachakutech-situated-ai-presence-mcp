# Voice at the desk

Status: exploration. This is not a ticket. C8–C13 do not wait on it. B1 does not include a microphone. Written 31 October 2026, after C7b (`d4a46e5`) and the preferences read in `research_tours.md` section 1a.

The product, for the person at the glass, is one of two uses of the same desk. A client model uses the layer as a review of documents, plans, and lessons, and it reads the decisions afterwards so it does not have to spend tokens on the pages. Or the person runs the local model and does the scientific review, the decision, and the investigation here. In both uses the microphone is a control at the human-computer interface, in the same family as the checkbox.

The fear that kept voice narrow was an unbound model between perception and presence, with the compute shaders in reach. A highlight and a presence that change only through the policy layer, or through a projection of that policy in the perceptual-memory stack, stay inside the design. That shape is a proposal crossing the gate. It is a different shape from perception feeding a model that writes the presence directly. Subsymbolic control of the splats remains a wanted later capability. Section 7 says what would have to exist before a latent may move a cloud.

The desk itself is the data layer of an editor's desktop. The person selects images and text, with a strong inverse for each selection, to carry into a document that may cite them. Layout is a later GUI. A false command is acceptable because the inverse is one event: uncheck, delete the note, or discard. A missed command is silence, and silence is the default.

## 1. The B branch, as it stands

| Ticket | What it is | On disk now |
|---|---|---|
| B1 | Sidecar client. Connects to `sidecar.sock` as T3. Schema-locked JSON mapped through the gate. | The socket, the tier, and the allow-list are in the daemon. Nothing connects. `observePhrase` is T3 and cannot express `acknowledged`. `draftTask` is T3. `createTask` on that socket is refused. `createProject` from T3 is a draft until T0 confirms. |
| B2 | Crop ingest at decision cadence, not at frame rate. | No crop actor. Screen text is the perceptual-memory actor that exists. |
| B3 | `presence doctor`, the shared model config, CPU placement ahead of the disc submits (`B3_doctor.md`). | No endpoint is read. The Iris Xe budget in the tour plan is still one 4-bit job, off the overlay thread. The doctor reports that and does not start the server. |
| B4, later | The same tier inside the process. Typed yes/no head. A learned advisor that may only tighten. | Not started. Moving the weights into the daemon does not change the tier. |

B1 is the import of the local model as a reflex specialist. Its job in the research tour is the one C10 already names: a yes/no on a short span, and an optional stance with a confidence. Stance never keeps. With the sidecar down the default judge keeps every shortlisted span, stance stays `unknown`, and the tour still runs. The citation walk and the preferences cursor do not block on B1.

The sketch in `local_multimodal_reflex.md` gives the sidecar three outputs: `observe` (`shown` or `dwelled` only), `draftTask`, and a `speak` string. `AvatarSpeak` and `AnimatePresence` are T2. The `speak` string is narration the tour runner may say. It is not a socket kind the sidecar sends, and it is not a write into the expand buffers. Acceptance on that plan already says a sidecar failure leaves no partial write in those buffers. B1 should keep that line. A free mouth on T3 would be a presence-layer write from the model.

B1, B2, and B3 stay pixel and text. Audio is not a crop. One process may later host the weights used here and the weights used for transcription. The tier is the socket the line is written to. A judgement is a value returned to the caller. `observePhrase`, `draftTask`, and a draft `createProject` are the lines that write `sidecar.sock`. The brief is `B1_sidecar.md`. A command from the user's mouth writes `command.sock`, and only after the parse in section 3.

## 2. What the voice ban is, and what it is worth

The command layer separates three paths that share the word "voice."

**Room audio into the substrate.** Tier T4. Perception is data. A page, a video, or someone across the room saying "approve all" cannot create a task, keep a locus, or move the presence. There is no microphone ingress actor. Speech playback is not that actor (`docs/architecture.md`). This ban is the one that still earns its keep: other people, playback, the avatar hearing herself, and a recording that would otherwise outlive the session.

**Push-to-talk, parsed as an intent on the command channel.** Tier T1, and the key that arms it is T0. The design already allows this. It was specified as speech converted to text, then parsed as a typed intent. C4 implemented the parser and refused the device: a "transcript" is stdin. `PRESENCE_VOICE_ANSWERS_APPROVE` is not consulted. Ambient audio is not read. `addNote` accepts `via: voice`, and the CLI can set that flag, and no capture fills the text. So the product has no voice path at all. That is stricter than the design. The missing piece is the armed command, not a hole in the ban on always-on collection.

**A model writing shaders.** Forbidden, and separate from the microphone. Messages describe. Commands name regions and ids. They do not name bones, splats, or buffers. The client never authors bones. The expand pass and the projection shader stay on the daemon's side of the gate.

Always-on collection would spend the first ban to imitate the second path, and it would put samples next to the third. The tool set is small enough that an armed command covers the desk. It is not small enough to make an open mic safe. A false "keep" is one uncheck. An open mic is a second person in the room with a pen.

## 3. A future function: an armed command

Worth remembering while C8–C13 and B1 are built. Not scheduled in front of them.

Capture, when it exists, belongs to an ingress actor in the perception runtime. The vision doc already names a microphone ingress actor beside screen and camera. The actor takes samples. It does not dispatch, and it does not run on the overlay thread. The recognizer stays off the frame for the same reason B1's inference stays off the frame.

The microphone is closed until the user arms it. The arming is physical, the same family as the note button and the note hotkey: a held key, or a click that means "listening," on the item under the pointer or the active item. When the arming ends, capture ends. A wake word is a small always-on detector. It is an open question in section 5, and it is not part of this function.

While the arming is held, audio is transcribed. The transcript is data until a grammar maps it onto a closed set. The local model may help find the boundary of a command. It does not receive a tool list. Language-following lands on the enum. A sentence outside the set is dropped. During "leave a note," the words after the command are the note, and they are not interpreted again.

The set, matched to the desk:

| Utterance | What it writes | Undo |
|---|---|---|
| leave a note, … | T1 `addNote`, `via: voice`, on the phrase the note arming already names. The words after the command are the text. C7b already stores that and already refuses T2 and T3. | `deleteNote` |
| keep, or check | See section 5. The conservative landing is an armed T0 acknowledge on that one item, the same flag a checkbox writes. | uncheck, which writes `cleared` and drops the keep |
| yes, or no | The approve pair, armed T0, one item. | the other button, which appends the opposite decision |
| skip, back, pause, resume, next source, previous source | The `UiEvent::Hotkey` values that already exist. | the opposite hotkey, or stop |
| stop | Dismiss the task. Items stay as they were. | the ledger is untouched, which is the current Esc rule |

There is no plural. "Approve all" as a command is outside the set. "Approve all" inside a note is the user's sentence, stored as text, and it grants nothing. A command does not open a URL, does not invent a move, and does not follow a link. A request to move still goes through `ViewerMove`: `WebFind`, `PdfPage`, or the 80 px nudge, under the same start rules as a click.

The outer client does not sit on this path. It reads `listPhraseFlags` and, once C13 exists, `queryProject`. That is the token saving. The audio never enters its context. The local model, when the person is using it for the investigation, may be the transcriber and the yes/no judge. Those roles stay on different sockets even if one process loaded the weights.

A false command is a type I error in the ordinary sense: the desk acts on a word the user did not mean. The inverse events above are the undo. They are per event, not a timeline scrubber, and there is no `undo` verb on the CLI today. A spoken "undo" would be a later alias for `cleared` on the latest keep of this phrase, or `deleteNote` on the latest note, and it should be one of those two rather than a new stack. A missed command is a type II error. The mic being closed makes a miss into silence, which is the bias this desk wants. The user has said the false command is the acceptable one.

Export follows the store. C12 rebuilds the bib, the tex, and the txt from keeps. A false keep is in the files until the user unchecks and exports again. The decisions csv can later carry `via: voice` beside `by`. The citation text does not need to look different.

## 4. How this holds the design

The two planes stay. Samples are T4 until a grammar emits one command. The command crosses at the gate as a proposal whose tier is the arming, not the model's opinion of its own tier.

Highlight geometry, the disc, and the splat buffers stay behind that gate. A voice line does not name a bone, a splat, or a shader uniform.

The direct graph, perception into the model into the presence, is the long arc in `docs/vision.md`. This function does not build it. The model sees a transcript of an armed moment, or a crop at decision cadence, and it returns a proposal. Presence actors manifest what the gate allows. Remove the model and the checkbox, the note, and the tour still work. That is the sidecar-fails-soft rule, applied to the ear.

A projection of the policy into perceptual memory is the later shape in section 7. Until that reader exists, both B1 and this command stop at proposals.

## 5. Open questions

These stay open on purpose. An implementation that answers them by defaulting to always-on, or by handing the sidecar the sample, has left this note.

1. **Wake word or key.** A key keeps the device in the user's hand and matches the note arming. A wake word needs a detector that is listening when the command mic is supposed to be closed. Shared offices make the key look insufficient, and the answer can remain "the key is the permission" until a room requires more. Speaker identity is a larger model and a privacy cost this note does not take.

2. **Does "keep" write the flag.** The design rule is that the user's body answers, and T0 is the only tier that writes `acknowledged`, approved, or discarded. A held key plus a spoken keep is the body if the arming is the witness and the word chooses which witness. It is not the body if only the click counts. Letting T1 or T3 write the flag would break the rule that those tiers do not answer. The landing to keep in mind is an extension of the T0 witness: armed voice, `by: T0`, with a marker that a later reader can tell from a click. Expired arming drops the words. The marker matters because the preferences cursor in `research_tours.md` section 1a is what the client model observes. A voice keep should show up there as a decision, and it should be honest about how it was made.

3. **Where the recognizer lives.** An ingress actor matches the vision doc. A helper process matches B1's phase 0 and keeps the recognizer off the overlay thread. Either is compatible with this note. One process that holds the microphone and also holds T3 is the unleashed path in miniature if a bug forwards room audio as `observePhrase` or `draftTask`. The capture service emits an enum on `command.sock`. The judge never sees the sample.

4. **Hearing herself.** Narration plays through the session audio path. An arming that overlaps a clip will transcribe the claim she just spoke. The command set is ignored while the speech board is playing. `docs/plans/complete/text_to_cues.md` already refuses to feed speaker output into microphone perception. The same cut applies here, at the scale of one sentence.

5. **The split between command and note text.** "Leave a note, cut the second clause" stores the clause. A grammar that takes the words after the verb cannot be redirected by a sentence on the page. A model judgement of the split can. Prefer the grammar. If the grammar misses, the result is still either one command or one note.

6. **The buffer.** A keep does not need the audio after the event is written. A note persists because the user asked to store words. Audio does not go into the project store, `events.jsonl`, the doc map, or a crop the sidecar can read. The open part is the transient buffer: tmpfs, wiped when the arming ends. A "last voice command" field on `queryProject` would become a second archive. The events are the read.

7. **The outer model.** Both products share the desk. The client receives the decision and the note, which it was going to read anyway. It does not receive the wav, and it does not receive a transcript of commands that were dropped. The local model may hear, because the person is talking to the desk.

8. **Frame budget.** One 4-bit job is the budget beside the discs. A recognizer that runs only while armed shares that budget in time rather than always. If a future choice is between an always-resident recognizer and the judge, the armed-only rule wins. The privacy failure and the frame failure are the same failure.

9. **False keeps in the file.** The bib contains a voice keep until uncheck and a fresh export. That is the undo. Whether `decisions.csv` names `voice` beside `by` is a column, not a reason to block the command. C12's escaping treats a voice note the same as a typed note.

## 6. What not to paint into a corner

While C8–C13 and B1 are in progress:

- T3 does not gain `addNote`, and it does not gain `acknowledged`. The voice note is the T1 path C7b shipped. A spoken keep, if it is ever built, is an armed T0 witness.
- B1's `speak` string stays off `AvatarSpeak`. Narration stays with the tour runner.
- `via: voice` is an origin label. Exports escape it with the same rules as typed text.
- The note arming, sixty seconds, one phrase, clicked item or active item, is the hook "leave a note" would consume. New tour hotkeys stay `UiEvent::Hotkey` values. The overlay does not grow a key grab.
- `queryProject` decisions, as section 1a of `research_tours.md` requires, are the read the client already needs. A voice decision would be one more row of that read.
- B2's crops stay pixels (`B2_crops.md`). No shader uniform is added for a model latent.

## 7. Splats, later

The wanted end is the local model having subsymbolic control of the cloud. The shaders today take a bone palette and finished splats from the daemon's expand pass. Nothing in the proposal enum names a splat. `MAX_BONES` and the projection shader are not a side door for a model.

The shape that matches both the vision doc and the command layer is a write into perceptual memory, then a presence actor that manifests only what a projected policy allows. The projection is the gate, looking at a manifestation record (highlight, point, breathe, a later named gesture). The latent is not the thing dispatched. A latent that reaches `avatar_expand.comp` or the projection shader is the outcome this note is trying to keep off the table.

Until that reader exists, "the model moved the cloud" is out of scope for B1 and for the voice command. Remembering the reader is enough. Building a uniform ahead of it is how the fear arrives early.

## 8. The hour at the desk

Select mode, wait pace, one passage highlighted. The user holds the key and says "keep." The checkbox takes the mark. They say "leave a note, cut the second clause." The note is on that locus. They say "next." The tour advances. They uncheck, or they say the undo alias once it exists. The keep is gone on the next read, and the next export leaves it out of the bib.

The client model, asked later, calls `queryProject` and sees the keeps, the decisions, and the note. It did not hear the room. The local model, if it judged the passage, did so as T3 stance and did not write the keep.

The key up, the same sentences do nothing.
