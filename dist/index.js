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
    title: "Highlight something on screen",
    description: "Ephemeral Manifestation. Draws a bounded, temporary highlight over the " +
        "described on-screen content. Prefer this over describing a location in " +
        "text when the user can see their screen.",
    inputSchema: {
        description: z.string().describe("What to highlight, e.g. 'the red error banner'"),
        durationSeconds: z.number().min(1).max(30).default(6),
    },
}, async ({ description, durationSeconds }) => {
    policy.assertAllowed("manifestHighlight");
    const result = await (await getDaemon()).highlightRegion(description, durationSeconds);
    return { content: [{ type: "text", text: JSON.stringify(result) }] };
});
server.registerTool("spawnPresence", {
    title: "Spawn a persistent presence",
    description: "Instanced Manifestation, step 1 of 3 (spawn / animate / retire). Creates a " +
        "persistent presence derived from the given context (e.g. 'a character based " +
        "on what's on screen'). Optionally built from a held artifact (see addArtifact) " +
        "rather than derived fresh. Returns a presenceId — pass it to animatePresence " +
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
    description: "Instanced Manifestation, step 2 of 3. Feeds new content (e.g. words to say) " +
        "to a presence created by spawnPresence.",
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
