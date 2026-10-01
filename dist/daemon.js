// Shared Presence-daemon surface. The MCP Binding talks only to this.
// `daemonStub` is the notify-send stand-in; `daemonClient` is the Unix-socket
// client to `presence-daemon`. `connectDaemon` prefers the socket and falls
// back to the stub so an agent CLI still works when the native process isn't
// running.
export {};
