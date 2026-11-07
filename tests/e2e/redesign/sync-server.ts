// A local rustysound-sync for the remote-sync specs. The sync server lives in its own
// (private) repository, so the specs skip unless one of these points at it:
//   RS_SYNC_BIN   a built `rustysound-sync` binary, or
//   RS_SYNC_DIR   a checkout, run with `cargo run --release -p rustysound-sync`
//                 (RS_SYNC_TARGET_DIR: its target dir, default <checkout>/target).
// Each worker gets its own port (4790 + worker), cache dir and rooms; logins are checked
// against the mock server. Never point the specs at a real sync server.
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const SYNC_BASE_PORT = Number(process.env.RS_SYNC_PORT ?? 4790);

export function syncServerAvailable(): boolean {
  return Boolean(process.env.RS_SYNC_BIN || process.env.RS_SYNC_DIR);
}

export interface SyncServer {
  url: string;
  stop(): Promise<void>;
  /** Starts it again after `stop` (same port and cache). */
  start(): Promise<void>;
  /** Stops it and removes its cache. */
  dispose(): Promise<void>;
}

async function healthy(url: string): Promise<boolean> {
  try {
    const r = await fetch(`${url}/health`, { signal: AbortSignal.timeout(1000) });
    return r.ok;
  } catch {
    return false;
  }
}

/** Starts a sync server on `port` that accepts logins for `allowed` (Subsonic base URLs). */
export async function startSyncServer(port: number, allowed: string[]): Promise<SyncServer> {
  const url = `http://127.0.0.1:${port}`;
  const cacheDir = mkdtempSync(join(tmpdir(), "rs-sync-cache-"));
  let child: ChildProcess | null = null;
  let output = "";

  const launch = async () => {
    // Another server on this port would quietly stand in with the wrong allow-list.
    if (await healthy(url)) throw new Error(`something is already serving ${url}; stop it or set RS_SYNC_PORT`);
    const env = {
      ...process.env,
      PORT: String(port),
      HOST: "127.0.0.1",
      ALLOWED_SERVERS: allowed.join(","),
      CACHE_DIR: cacheDir,
      LRCLIB_URL: "off",
      RUST_LOG: "warn",
    };
    child = process.env.RS_SYNC_BIN
      ? spawn(process.env.RS_SYNC_BIN, [], { env, stdio: ["ignore", "pipe", "pipe"] })
      : spawn("cargo", ["run", "--release", "-q", "-p", "rustysound-sync"], {
          cwd: process.env.RS_SYNC_DIR,
          env: { ...env, CARGO_TARGET_DIR: process.env.RS_SYNC_TARGET_DIR ?? join(process.env.RS_SYNC_DIR!, "target") },
          stdio: ["ignore", "pipe", "pipe"],
        });
    child.stdout?.on("data", (d) => (output += d));
    child.stderr?.on("data", (d) => (output += d));
    const started = Date.now();
    while (Date.now() - started < 120_000) {
      if (await healthy(url)) return;
      if (child.exitCode !== null) throw new Error(`rustysound-sync exited (${child.exitCode}):\n${output}`);
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error(`rustysound-sync did not come up at ${url}:\n${output}`);
  };

  const stop = async () => {
    const running = child;
    child = null;
    if (!running || running.exitCode !== null) return;
    const exited = new Promise((r) => running.once("exit", r));
    running.kill("SIGTERM");
    await Promise.race([exited, new Promise((r) => setTimeout(r, 7000))]);
    if (running.exitCode === null) running.kill("SIGKILL");
    // `cargo run` forwards the signal; wait until the port is really free.
    const started = Date.now();
    while ((await healthy(url)) && Date.now() - started < 10_000) await new Promise((r) => setTimeout(r, 100));
  };

  await launch();
  return {
    url,
    stop,
    start: launch,
    dispose: async () => {
      await stop();
      rmSync(cacheDir, { recursive: true, force: true });
    },
  };
}
