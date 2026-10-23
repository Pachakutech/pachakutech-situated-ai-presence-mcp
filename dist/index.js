#!/usr/bin/env node
// The MCP Binding for the Pachakutech Presence Layer.
// Six Manifestations: one ephemeral, one instanced presence lifecycle
// (spawn/animate/retire), one instanced artifact lifecycle (add/retire).
// See README.md and skills/presence/SKILL.md.
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { PolicyGate } from "./policyGate.js";
import { daemonStub } from "./daemonStub.js";
import { tryConnectDaemon } from "./daemonClient.js";
import { locateRegion } from "./locate/run.js";
const policy = new PolicyGate();
const server = new McpServer({ name: "pachakutech-presence", version: "0.1.0" });
let daemon = daemonStub;
let daemonReady = null;
function getDaemon() {
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
server.registerTool("manifestHighlight", {
    title: "Highlight a box on screen",
    description: "Ephemeral. Tints a full-screen UV box for a live presence: origin top-left, " +
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
}, async ({ presenceId, description, u, v, w, h, durationSeconds }) => {
    policy.assertAllowed("manifestHighlight");
    policy.assertInstanceExists(presenceId);
    const result = await (await getDaemon()).highlightRegion(presenceId, description, u, v, w, h, durationSeconds);
    return { content: [{ type: "text", text: JSON.stringify(result) }] };
});
server.registerTool("spawnPresence", {
    title: "Spawn a persistent presence",
    description: "Instanced Manifestation, step 1 of 3 (spawn / animate / retire). Creates a " +
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
}, async ({ sourceContext, styleHint, artifactId }) => {
    policy.assertAllowed("spawnPresence");
    if (artifactId)
        policy.assertArtifactExists(artifactId);
    const presenceId = `p-${Math.random().toString(36).slice(2, 8)}`;
    policy.registerInstance(presenceId);
    await (await getDaemon()).spawnPresence(presenceId, sourceContext, styleHint, artifactId);
    return { content: [{ type: "text", text: JSON.stringify({ presenceId }) }] };
});
server.registerTool("addArtifact", {
    title: "Add a reference artifact",
    description: "Instanced Manifestation, step 1 of 2 (add / retire). Adds a Gaussian splat " +
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
            .describe("Local filesystem path to a .splat or .ply Gaussian cloud. The daemon reads this file; it does not fetch URLs. Omit only if you have no file yet — that holds an empty cloud."),
    },
}, async ({ description, sourceUri }) => {
    policy.assertAllowed("addArtifact");
    const artifactId = `a-${Math.random().toString(36).slice(2, 8)}`;
    policy.registerArtifact(artifactId);
    await (await getDaemon()).addArtifact(artifactId, description, sourceUri);
    return { content: [{ type: "text", text: JSON.stringify({ artifactId }) }] };
});
server.registerTool("retireArtifact", {
    title: "Retire a reference artifact",
    description: "Instanced Manifestation, step 2 of 2. Removes a held artifact and frees its slot.",
    inputSchema: { artifactId: z.string() },
}, async ({ artifactId }) => {
    policy.assertAllowed("retireArtifact");
    policy.retireArtifact(artifactId);
    await (await getDaemon()).retireArtifact(artifactId);
    return { content: [{ type: "text", text: JSON.stringify({ status: "retired" }) }] };
});
server.registerTool("animatePresence", {
    title: "Drive an existing presence",
    description: "Instanced Manifestation, step 2 of 3. Feeds new content to a presence created " +
        "by spawnPresence. The text is spoken aloud by that presence (local voice, " +
        "mouth synced to the audio); a new call interrupts the previous utterance. " +
        "Errors if the presence has no avatar body.",
    inputSchema: {
        presenceId: z.string(),
        text: z.string().describe("What the presence should say or express next"),
    },
}, async ({ presenceId, text }) => {
    policy.assertAllowed("animatePresence");
    policy.assertInstanceExists(presenceId);
    await (await getDaemon()).animatePresence(presenceId, text);
    return { content: [{ type: "text", text: JSON.stringify({ status: "ok" }) }] };
});
server.registerTool("locateScreenRegion", {
    title: "Find a phrase on screen",
    description: "Returns full-screen UV for a phrase on a web page, PDF, terminal, or document window. " +
        "One process screenshots and reads the words. It does not mark and it does not speak. " +
        "Web can open a URL and scroll. PDF opens Evince from a file path and can turn to `page`. " +
        "Terminal and doc read a window already up; they do not type, and they do not ask a multiplexer. " +
        "Prefer this over a hand-rolled grim and tesseract pipeline. A passage query is `start ... end`.",
    inputSchema: {
        source: z.enum(["web", "pdf", "terminal", "doc", "ide"]).describe("Which window to read"),
        query: z.string().describe("Phrase, or `start ... end` for a passage"),
        uri: z.string().optional().describe("Web URL, or a local path for pdf and doc"),
        page: z.number().int().min(1).max(9999).optional().describe("PDF page, starting at 1"),
        pad: z.number().int().min(0).max(64).optional().describe("Pixels around the matched lines. Default 12"),
        timeoutS: z.number().min(1).max(120).optional().describe("Seconds to wait. Default 15"),
        noOpen: z.boolean().optional().describe("Measure a window that is already up"),
        windowClass: z.string().optional().describe("Case-insensitive regular expression on the window class"),
        windowTitle: z.string().optional().describe("Case-insensitive regular expression on the window title"),
        verify: z.boolean().optional().describe("Crop-check the box before returning it"),
    },
}, async ({ source, query, uri, page, pad, timeoutS, noOpen, windowClass, windowTitle, verify }) => {
    policy.assertAllowed("locateScreenRegion");
    const result = await locateRegion({
        source,
        query,
        uri,
        page,
        pad: pad ?? 12,
        timeoutS: timeoutS ?? 15,
        noOpen: Boolean(noOpen),
        verify: Boolean(verify),
        windowClass,
        windowTitle,
    });
    return { content: [{ type: "text", text: JSON.stringify(result) }] };
});
server.registerTool("retirePresence", {
    title: "End a presence",
    description: "Instanced Manifestation, step 3 of 3. Cleanly ends a presence created by " +
        "spawnPresence and frees its slot. Always call this when the presence is no " +
        "longer needed — concurrent live presences are capped.",
    inputSchema: { presenceId: z.string() },
}, async ({ presenceId }) => {
    policy.assertAllowed("retirePresence");
    policy.retireInstance(presenceId);
    await (await getDaemon()).retirePresence(presenceId);
    return { content: [{ type: "text", text: JSON.stringify({ status: "retired" }) }] };
});
async function main() {
    const transport = new StdioServerTransport();
    await server.connect(transport);
}
main().catch((error) => {
    console.error("Fatal error starting presence MCP server:", error);
    process.exit(1);
});
