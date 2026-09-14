// The Policy Gate: every Manifestation passes through here before it touches
// the substrate (today: the daemon stub; eventually: the native Vulkan daemon).
// This is intentionally small and readable — it is the one piece of this repo
// that should get *more* conservative as capability grows, not less.
// Two independent registries with independent caps. Presences and artifacts
// are different risk classes — an artifact is inert content sitting in the
// substrate; a presence is something actively rendered — so they're capped
// and reasoned about separately even though the lifecycle shape is the same.
export class PolicyGate {
    lastCallAt = new Map();
    livePresences = new Map();
    liveArtifacts = new Map();
    static MIN_INTERVAL_MS = 1500;
    static MAX_CONCURRENT_PRESENCES = 3;
    static MAX_CONCURRENT_ARTIFACTS = 20;
    assertAllowed(action) {
        const last = this.lastCallAt.get(action) ?? 0;
        const now = Date.now();
        if (now - last < PolicyGate.MIN_INTERVAL_MS) {
            throw new Error(`${action} rate-limited: called again too soon`);
        }
        this.lastCallAt.set(action, now);
    }
    registerInstance(presenceId) {
        if (this.livePresences.size >= PolicyGate.MAX_CONCURRENT_PRESENCES) {
            throw new Error(`Refusing to spawn: ${PolicyGate.MAX_CONCURRENT_PRESENCES} presences already live. Retire one first.`);
        }
        this.livePresences.set(presenceId, { id: presenceId, createdAt: Date.now() });
    }
    assertInstanceExists(presenceId) {
        if (!this.livePresences.has(presenceId)) {
            throw new Error(`No live presence with id ${presenceId}`);
        }
    }
    retireInstance(presenceId) {
        this.assertInstanceExists(presenceId);
        this.livePresences.delete(presenceId);
    }
    // Artifacts are a content checkpoint even though nothing renders them yet:
    // they're agent-authored data that a presence may later be built from, so
    // this is the right place to eventually add size/format/provenance checks.
    registerArtifact(artifactId) {
        if (this.liveArtifacts.size >= PolicyGate.MAX_CONCURRENT_ARTIFACTS) {
            throw new Error(`Refusing to add artifact: ${PolicyGate.MAX_CONCURRENT_ARTIFACTS} already held. Retire one first.`);
        }
        this.liveArtifacts.set(artifactId, { id: artifactId, createdAt: Date.now() });
    }
    assertArtifactExists(artifactId) {
        if (!this.liveArtifacts.has(artifactId)) {
            throw new Error(`No held artifact with id ${artifactId}`);
        }
    }
    retireArtifact(artifactId) {
        this.assertArtifactExists(artifactId);
        this.liveArtifacts.delete(artifactId);
    }
}
