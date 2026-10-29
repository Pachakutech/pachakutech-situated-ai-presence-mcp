#!/usr/bin/env node
// The MCP Binding for the Pachakutech Presence Layer.
// Highlight, spawn / animate / retire, artifacts, locate, and the fused
// find-tint-speak pair. See README.md and skills/presence/SKILL.md.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { PolicyGate } from "./policyGate.js";
import { PresenceDaemon } from "./daemon.js";
import { daemonStub } from "./daemonStub.js";
import { tryConnectDaemon } from "./daemonClient.js";
import { BOX_PAD } from "./locate/core.js";
import { fuseRegion, locateRegion } from "./locate/run.js";

const policy = new PolicyGate();
const server = new McpServer({ name: "pachakutech-presence", version: "0.1.0" });

let daemon: PresenceDaemon = daemonStub;
let daemonReady: Promise<void> | null = null;

function getDaemon(): Promise<PresenceDaemon> {
  if (!daemonReady) {
    daemonReady = tryConnectDaemon()
      .then((client) => {
        daemon = client;
        console.error("[presence] connected to presence-daemon");
      })
      .catch(() => {
        daemon = daemonStub;
        console.error("[presence] no presence-daemon socket; using notify-send stub");
      });
  }
  return daemonReady.then(() => daemon);
}

server.registerTool(
  "manifestHighlight",
  {
    title: "Highlight a box on screen",
    description:
      "Ephemeral. Tints a full-screen UV box for a live presence: origin top-left, " +
      "u and v from 0 to 1 across the whole output, v growing downward. The mark fades " +
      "after durationSeconds (default 5, max 360); call again to replace the box and keep it up. " +
      "The box remains that presence's area of interest after the pixels fade, until a new box or retire. " +
      "The figure does not gesture yet.",
    inputSchema: {
      presenceId: z.string().describe("Live presence this box belongs to"),
      description: z.string().describe("Short note for the log, e.g. 'the red error banner'"),
      u: z.number().min(0).max(1).describe("Left edge, 0..1 across the whole screen"),
      v: z.number().min(0).max(1).describe("Top edge, 0..1, growing downward"),
      w: z.number().positive().describe("Width in that same 0..1 space"),
      h: z.number().positive().describe("Height in that same 0..1 space"),
      durationSeconds: z.number().min(1).max(360).default(5),
    },
  },
  async ({ presenceId, description, u, v, w, h, durationSeconds }) => {
    policy.assertAllowed("manifestHighlight");
    policy.assertInstanceExists(presenceId);
    const result = await (await getDaemon()).highlightRegion(
      presenceId,
      description,
      u,
      v,
      w,
      h,
      durationSeconds,
    );
    return { content: [{ type: "text", text: JSON.stringify(result) }] };
  },
);

server.registerTool(
  "spawnPresence",
  {
    title: "Spawn a persistent presence",
    description:
      "Instanced Manifestation, step 1 of 3 (spawn / animate / retire). Creates a " +
      "persistent presence derived from the given context (e.g. 'a character based " +
      "on what's on screen'). Optionally built from a held artifact (see addArtifact) " +
      "rather than derived fresh. The first live presence gets the daemon's avatar body. " +
      "Returns a presenceId — pass it to animatePresence " +
      "and retirePresence. The generative work (what the presence actually looks " +
      "like) happens inside this contract; the contract itself never changes.",
    inputSchema: {
      sourceContext: z.string().describe("What to derive the presence from"),
      styleHint: z.string().optional().describe("Optional style guidance"),
      artifactId: z
        .string()
        .optional()
        .describe("Optional id from addArtifact — use this artifact's content as the basis"),
    },
  },
  async ({ sourceContext, styleHint, artifactId }) => {
    policy.assertAllowed("spawnPresence");
    if (artifactId) policy.assertArtifactExists(artifactId);
    const presenceId = `p-${Math.random().toString(36).slice(2, 8)}`;
    policy.registerInstance(presenceId);
    await (await getDaemon()).spawnPresence(presenceId, sourceContext, styleHint, artifactId);
    return { content: [{ type: "text", text: JSON.stringify({ presenceId }) }] };
  },
);

server.registerTool(
  "addArtifact",
  {
    title: "Add a reference artifact",
    description:
      "Instanced Manifestation, step 1 of 2 (add / retire). Adds a Gaussian splat " +
      "cloud to the substrate as reference material — it is held, not rendered. " +
      "Use this to give spawnPresence something concrete to build from (e.g. " +
      "'this is what I want the presence to look like') rather than describing " +
      "appearance in prose. Splat clouds only for now — no mesh/texture formats " +
      "(glTF, etc.); converting those is a separate, unbuilt concern. Returns an " +
      "artifactId.",
    inputSchema: {
      description: z.string().describe("What this splat cloud is/depicts"),
      sourceUri: z
        .string()
        .optional()
        .describe(
          "Local filesystem path to a .splat or .ply Gaussian cloud. The daemon reads this file; it does not fetch URLs. Omit only if you have no file yet — that holds an empty cloud.",
        ),
    },
  },
  async ({ description, sourceUri }) => {
    policy.assertAllowed("addArtifact");
    const artifactId = `a-${Math.random().toString(36).slice(2, 8)}`;
    policy.registerArtifact(artifactId);
    await (await getDaemon()).addArtifact(artifactId, description, sourceUri);
    return { content: [{ type: "text", text: JSON.stringify({ artifactId }) }] };
  },
);

server.registerTool(
  "retireArtifact",
  {
    title: "Retire a reference artifact",
    description:
      "Instanced Manifestation, step 2 of 2. Removes a held artifact and frees its slot.",
    inputSchema: { artifactId: z.string() },
  },
  async ({ artifactId }) => {
    policy.assertAllowed("retireArtifact");
    policy.retireArtifact(artifactId);
    await (await getDaemon()).retireArtifact(artifactId);
    return { content: [{ type: "text", text: JSON.stringify({ status: "retired" }) }] };
  },
);

server.registerTool(
  "animatePresence",
  {
    title: "Drive an existing presence",
    description:
      "Instanced Manifestation, step 2 of 3. Feeds new content to a presence created " +
      "by spawnPresence. The text is spoken aloud by that presence (local voice, " +
      "mouth synced to the audio); a new call interrupts the previous utterance. " +
      "Errors if the presence has no avatar body.",
    inputSchema: {
      presenceId: z.string(),
      text: z.string().describe("What the presence should say or express next"),
    },
  },
  async ({ presenceId, text }) => {
    policy.assertAllowed("animatePresence");
    policy.assertInstanceExists(presenceId);
    await (await getDaemon()).animatePresence(presenceId, text);
    return { content: [{ type: "text", text: JSON.stringify({ status: "ok" }) }] };
  },
);

server.registerTool(
  "locateScreenRegion",
  {
    title: "Find a phrase on screen",
    description:
      "Returns full-screen UV for a phrase on a web page, PDF, terminal, or document window. " +
      "One process screenshots and reads the words. It does not mark and it does not speak. " +
      "Web can open a URL and scroll. PDF opens Evince from a file path and can turn to `page`. " +
      "Terminal, doc, and ide read a window already up; they do not type, and they do not ask a multiplexer. " +
      "ide is the editor window (code, code-oss, cursor, zed, jetbrains). " +
      "Prefer this over a hand-rolled grim and tesseract pipeline. A passage query is `start ... end`.",
    inputSchema: {
      source: z.enum(["web", "pdf", "terminal", "doc", "ide"]).describe("Which window to read"),
      query: z.string().describe("Phrase, or `start ... end` for a passage"),
      uri: z.string().optional().describe("Web URL, or a local path for pdf and doc"),
      page: z.number().int().min(1).max(9999).optional().describe("PDF page, starting at 1"),
      pad: z.number().int().min(0).max(64).optional().describe("Extra pixels around the matched text, only when passed. Default 0"),
      timeoutS: z.number().min(1).max(120).optional().describe("Seconds to wait. Default 15"),
      noOpen: z.boolean().optional().describe("Measure a window that is already up"),
      windowClass: z.string().optional().describe("Case-insensitive regular expression on the window class"),
      windowTitle: z.string().optional().describe("Case-insensitive regular expression on the window title"),
      verify: z.boolean().optional().describe("Crop-check the box before returning it"),
    },
  },
  async ({ source, query, uri, page, pad, timeoutS, noOpen, windowClass, windowTitle, verify }) => {
    policy.assertAllowed("locateScreenRegion");
    const result = await locateRegion({
      source,
      query,
      uri,
      page,
      pad: pad ?? BOX_PAD,
      timeoutS: timeoutS ?? 15,
      noOpen: Boolean(noOpen),
      verify: Boolean(verify),
      windowClass,
      windowTitle,
    });
    return { content: [{ type: "text", text: JSON.stringify(result) }] };
  },
);

const regionFields = {
  source: z.enum(["web", "pdf", "terminal", "doc", "ide"]).describe("Which window to read"),
  query: z.string().describe("Phrase, or `start ... end` for a passage"),
  uri: z.string().optional().describe("Web URL, or a local path for pdf and doc"),
  page: z.number().int().min(1).max(9999).optional().describe("PDF page, starting at 1"),
  pad: z.number().int().min(0).max(64).optional().describe("Extra pixels around the matched text, only when passed. Default 0"),
  timeoutS: z.number().min(1).max(120).optional().describe("Seconds to wait. Default 15"),
  noOpen: z.boolean().optional().describe("Measure a window that is already up"),
  windowClass: z.string().optional().describe("Case-insensitive regular expression on the window class"),
  windowTitle: z.string().optional().describe("Case-insensitive regular expression on the window title"),
  durationSeconds: z.number().min(1).max(360).default(90).describe("How long the tint stays up"),
};

server.registerTool(
  "highlightAndSpeak",
  {
    title: "Find a phrase, tint it, and speak",
    description:
      "One call. Pass text, the spoken reply, and either one query or cues. " +
      "cues annotate that reply: each at is one sentence of text, and each query is the passage to find on the page. " +
      "The runtime splits text, binds the cues, finds the first passage before any audio, then speaks. " +
      "A cue covers its sentence and every sentence until the next cue, and the same query keeps that highlight. " +
      "Those sentences start once the tint is up. When they have been said, the page scrolls and the next tint goes up, and then those sentences begin. " +
      "Inside one passage the sentences follow on. The tint comes down while the page moves and goes up when the crop passes. " +
      "It stays for the passage, and its safety cap is about as long as the words. A miss on the first passage does not tint and does not speak. " +
      "With no cues, the tint lasts durationSeconds (default 90, max 360). " +
      "One rate-limit tick. text is the spoken line, not a summary of the matched phrase. " +
      "On the web the browser's find moves the page. PDF opens Evince on the text-layer page, and an explicit page wins. Terminal, doc, and ide read a window already up. " +
      "ide is the editor window (code, code-oss, cursor, zed, jetbrains). Reveal the line. It does not type.",
    inputSchema: {
      presenceId: z.string().describe("Live presence from spawnPresence"),
      text: z.string().describe("The spoken line, the same words as the written reply"),
      ...regionFields,
      query: z.string().optional().describe("Phrase, or `start ... end` for a passage. Omit when cues are set"),
      cues: z
        .array(
          z.object({
            at: z.string().describe("Opening of one sentence of text. At least 16 characters, or the whole sentence"),
            query: z.string().describe("Words on the page to tint while that sentence plays. A phrase, or start ... end"),
          }),
        )
        .optional()
        .describe("Notes on text. Each names a sentence and the words to find on the page. Sentences until the next cue share that highlight. The same query stays on one place. The tint goes up before those sentences, and the page moves when they have been said"),
    },
  },
  async ({ presenceId, text, source, query, uri, page, pad, timeoutS, noOpen, windowClass, windowTitle, durationSeconds, cues }) => {
    policy.assertAllowed("highlightAndSpeak");
    policy.assertInstanceExists(presenceId);
    if (!cues?.length && !query?.trim()) {
      return {
        content: [{ type: "text", text: JSON.stringify({ ok: false, error: "bad_args", hint: "Pass query or cues." }) }],
      };
    }
    const result = await fuseRegion(
      {
        source,
        query: query ?? "",
        uri,
        page,
        pad: pad ?? BOX_PAD,
        timeoutS: timeoutS ?? 15,
        noOpen: Boolean(noOpen),
        verify: true,
        windowClass,
        windowTitle,
        seconds: durationSeconds,
        cues,
      },
      presenceId,
      text,
    );
    return { content: [{ type: "text", text: JSON.stringify(result) }] };
  },
);

server.registerTool(
  "manifestHighlightFromQuery",
  {
    title: "Find a phrase and tint it",
    description:
      "Finds the phrase, checks the crop, and tints it. No speech. " +
      "The tint lasts durationSeconds (default 90, max 360). A locate miss does not tint. " +
      "One rate-limit tick. On the web the browser's find moves the page. PDF opens Evince on the text-layer page, and an explicit page wins. " +
      "Terminal, doc, and ide read a window already up and do not type. " +
      "ide is the editor window (code, code-oss, cursor, zed, jetbrains).",
    inputSchema: {
      presenceId: z.string().describe("Live presence from spawnPresence"),
      ...regionFields,
    },
  },
  async ({ presenceId, source, query, uri, page, pad, timeoutS, noOpen, windowClass, windowTitle, durationSeconds }) => {
    policy.assertAllowed("manifestHighlightFromQuery");
    policy.assertInstanceExists(presenceId);
    const result = await fuseRegion(
      {
        source,
        query,
        uri,
        page,
        pad: pad ?? BOX_PAD,
        timeoutS: timeoutS ?? 15,
        noOpen: Boolean(noOpen),
        verify: true,
        windowClass,
        windowTitle,
        seconds: durationSeconds,
      },
      presenceId,
    );
    return { content: [{ type: "text", text: JSON.stringify(result) }] };
  },
);

server.registerTool(
  "listPhraseFlags",
  {
    title: "Read the phrase ledger",
    description:
      "Reads the daemon's phrase ledger, the list of phrases and the user's answers. " +
      "The ledger is empty when the daemon starts and is not kept across a restart. " +
      "acknowledged and decisions come from the user's hands; filter them with decidedBy T0. " +
      "mismatch and notFound mean the find failed. A client that lost its context reads this again. " +
      "It may propose a new phrase. It does not rebuild the list from memory.",
    inputSchema: {
      since: z.number().int().nonnegative().optional().describe("Exclusive event seq. Rows and events after this seq."),
      taskId: z.string().optional(),
      docKey: z.string().optional(),
      decidedBy: z.enum(["T0", "T1", "T2", "T3"]).optional().describe("Keep phrases whose decision was stamped by this tier. T0 is the user."),
      limit: z.number().int().nonnegative().optional().describe("Default 50. Values above 200 are clamped."),
      events: z.boolean().optional().describe("Return the event log instead of the derived rows."),
    },
  },
  async (query) => {
    policy.assertAllowed("listPhraseFlags");
    const detail = await (await getDaemon()).listPhraseFlags(query);
    return { content: [{ type: "text", text: JSON.stringify(detail) }] };
  },
);

const taskItem = z.object({
  phraseId: z.string().optional().describe("Existing phrase id. Omit when query is set."),
  query: z.string().optional().describe("Phrase text. Omit when phraseId is set."),
  narration: z.string().optional().describe("Spoken line for this item, cut at 300 characters."),
  scroll: z.boolean().optional().describe("The item is off the glass and will need a move."),
  page: z.number().int().positive().optional().describe("PDF page, starting at 1."),
});

server.registerTool(
  "createPresenceTask",
  {
    title: "Create a task",
    description:
      "Opens a task on the daemon. The daemon holds the list. " +
      "explain may draw nothing or a checkbox. select draws a checkbox. approve draws yes and no. " +
      "The client does not send origin, checked, or a decision. " +
      "A client that lost its context reads the task again with queryPresenceTask.",
    inputSchema: {
      presenceId: z.string().describe("Live presence the task belongs to"),
      mode: z.enum(["explain", "select", "approve"]),
      pace: z.enum(["flow", "wait"]).optional().describe("explain defaults to flow. select and approve default to wait."),
      controls: z.enum(["none", "check"]).optional().describe("explain defaults to none. select defaults to check. approve takes no controls value."),
      prompt: z.string().optional().describe("Agent text, cut at 120 characters."),
      ttlSeconds: z.number().int().positive().optional().describe("Seconds until the task expires. Default 120. Zero is rejected."),
      itemTimeoutSeconds: z.number().int().positive().optional().describe("Seconds an item may wait. Default 20."),
      docKey: z.string().optional(),
      items: z.array(taskItem).min(1).max(12),
    },
  },
  async (input) => {
    policy.assertAllowed("createPresenceTask");
    const detail = await (await getDaemon()).createTask(input);
    return { content: [{ type: "text", text: JSON.stringify(detail) }] };
  },
);

server.registerTool(
  "cancelPresenceTask",
  {
    title: "Dismiss a task",
    description: "Dismisses the task. Phrase rows stay as they are. The client's answers are not cleared.",
    inputSchema: { taskId: z.string() },
  },
  async ({ taskId }) => {
    policy.assertAllowed("cancelPresenceTask");
    const detail = await (await getDaemon()).cancelTask(taskId);
    return { content: [{ type: "text", text: JSON.stringify(detail) }] };
  },
);

server.registerTool(
  "queryPresenceTask",
  {
    title: "Read a task",
    description:
      "Reads the task the daemon holds: state, items, and whether a control is drawn. " +
      "The checkbox and the yes/no pair are the user's. A client does not write those answers. " +
      "After a compaction, read the task again.",
    inputSchema: { taskId: z.string() },
  },
  async ({ taskId }) => {
    policy.assertAllowed("queryPresenceTask");
    const detail = await (await getDaemon()).queryTask(taskId);
    return { content: [{ type: "text", text: JSON.stringify(detail) }] };
  },
);

server.registerTool(
  "retirePresence",
  {
    title: "End a presence",
    description:
      "Instanced Manifestation, step 3 of 3. Cleanly ends a presence created by " +
      "spawnPresence and frees its slot. Always call this when the presence is no " +
      "longer needed — concurrent live presences are capped.",
    inputSchema: { presenceId: z.string() },
  },
  async ({ presenceId }) => {
    policy.assertAllowed("retirePresence");
    policy.retireInstance(presenceId);
    await (await getDaemon()).retirePresence(presenceId);
    return { content: [{ type: "text", text: JSON.stringify({ status: "retired" }) }] };
  },
);

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((error) => {
  console.error("Fatal error starting presence MCP server:", error);
  process.exit(1);
});
