// Opens a source the project tour already named. The profile and port are the
// locate Chromium. A port that is already up is reused, so a second browser
// is not started on that profile.

export const LOCATE_PORT = 9222;
export const PDF_BIN = "evince";

export function locateProfile(env: NodeJS.ProcessEnv = process.env): string {
  const runtime = env.XDG_RUNTIME_DIR || "/tmp";
  return `${runtime}/pachakutech/locate-chromium`;
}

export function webArgs(url: string, env: NodeJS.ProcessEnv = process.env): string[] {
  return [
    "--ozone-platform=wayland",
    `--user-data-dir=${locateProfile(env)}`,
    `--remote-debugging-port=${LOCATE_PORT}`,
    "--remote-debugging-address=127.0.0.1",
    "--no-first-run",
    "--no-default-browser-check",
    "--new-window",
    url,
  ];
}

export function pdfArgs(file: string): string[] {
  return [file];
}

export type OpenDeps = {
  portOpen(): Promise<boolean>;
  spawn(bin: string, args: string[]): void;
  navigate(url: string): Promise<void>;
};

export async function openWeb(
  url: string,
  deps: OpenDeps,
  env: NodeJS.ProcessEnv = process.env,
): Promise<"reused" | "spawned"> {
  if (await deps.portOpen()) {
    await deps.navigate(url);
    return "reused";
  }
  deps.spawn(env.PRESENCE_BROWSER || "chromium", webArgs(url, env));
  return "spawned";
}

export function openPdf(file: string, deps: { spawn(bin: string, args: string[]): void }): void {
  deps.spawn(PDF_BIN, pdfArgs(file));
}

export async function launchListed(
  spec: { kind: "web"; url: string } | { kind: "pdf"; path: string },
  deps: OpenDeps,
): Promise<void> {
  if (spec.kind === "web") {
    await openWeb(spec.url, deps);
    return;
  }
  openPdf(spec.path, deps);
}
