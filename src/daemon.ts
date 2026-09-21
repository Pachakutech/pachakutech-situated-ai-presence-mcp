// Shared Presence-daemon surface. The MCP Binding talks only to this.
// `daemonStub` is the notify-send stand-in; `daemonClient` is the Unix-socket
// client to `presence-daemon`. `connectDaemon` prefers the socket and falls
// back to the stub so an agent CLI still works when the native process isn't
// running.

export type HighlightResult = {
  regionId: string;
  resolvedBounds: [number, number, number, number] | null;
};

export interface PresenceDaemon {
  highlightRegion(description: string, durationSeconds: number): Promise<HighlightResult>;
  spawnPresence(
    presenceId: string,
    sourceContext: string,
    styleHint?: string,
    artifactId?: string,
  ): Promise<void>;
  animatePresence(presenceId: string, text: string): Promise<void>;
  retirePresence(presenceId: string): Promise<void>;
  addArtifact(artifactId: string, description: string, sourceUri?: string): Promise<void>;
  retireArtifact(artifactId: string): Promise<void>;
}
