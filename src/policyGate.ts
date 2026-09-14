// The Policy Gate: every Manifestation passes through here before it touches
// the substrate (today: the daemon stub; eventually: the native Vulkan daemon).
// This is intentionally small and readable — it is the one piece of this repo
// that should get *more* conservative as capability grows, not less.

interface Record_ {
  id: string;
  createdAt: number;
}

// Two independent registries with independent caps. Presences and artifacts
// are different risk classes — an artifact is inert content sitting in the
// substrate; a presence is something actively rendered — so they're capped
// and reasoned about separately even though the lifecycle shape is the same.
export class PolicyGate {
  private lastCallAt = new Map<string, number>();
  private livePresences = new Map<string, Record_>();
  private liveArtifacts = new Map<string, Record_>();

  private static readonly MIN_INTERVAL_MS = 1500;
  private static readonly MAX_CONCURRENT_PRESENCES = 3;
  private static readonly MAX_CONCURRENT_ARTIFACTS = 20;

  assertAllowed(action: string) {
    const last = this.lastCallAt.get(action) ?? 0;
    const now = Date.now();
    if (now - last < PolicyGate.MIN_INTERVAL_MS) {
      throw new Error(`${action} rate-limited: called again too soon`);
    }
    this.lastCallAt.set(action, now);
  }

  registerInstance(presenceId: string) {
    if (this.livePresences.size >= PolicyGate.MAX_CONCURRENT_PRESENCES) {
      throw new Error(
        `Refusing to spawn: ${PolicyGate.MAX_CONCURRENT_PRESENCES} presences already live. Retire one first.`,
      );
    }
    this.livePresences.set(presenceId, { id: presenceId, createdAt: Date.now() });
  }

  assertInstanceExists(presenceId: string) {
    if (!this.livePresences.has(presenceId)) {
      throw new Error(`No live presence with id ${presenceId}`);
    }
  }

  retireInstance(presenceId: string) {
    this.assertInstanceExists(presenceId);
    this.livePresences.delete(presenceId);
  }

  // Artifacts are a content checkpoint even though nothing renders them yet:
  // they're agent-authored data that a presence may later be built from, so
  // this is the right place to eventually add size/format/provenance checks.
  registerArtifact(artifactId: string) {
    if (this.liveArtifacts.size >= PolicyGate.MAX_CONCURRENT_ARTIFACTS) {
      throw new Error(
        `Refusing to add artifact: ${PolicyGate.MAX_CONCURRENT_ARTIFACTS} already held. Retire one first.`,
      );
    }
    this.liveArtifacts.set(artifactId, { id: artifactId, createdAt: Date.now() });
  }

  assertArtifactExists(artifactId: string) {
    if (!this.liveArtifacts.has(artifactId)) {
      throw new Error(`No held artifact with id ${artifactId}`);
    }
  }

  retireArtifact(artifactId: string) {
    this.assertArtifactExists(artifactId);
    this.liveArtifacts.delete(artifactId);
  }
}
