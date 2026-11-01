// Starts the mock Subsonic server and a static server for the web build, unless they are
// already running (or RS_WEB_URL points at another server). Returns the teardown.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { MOCK_PORT, MOCK_URL, REPO_ROOT, WEB_DIR, WEB_PORT, WEB_URL } from "./env";

const BUN = process.env.BUN ?? "bun";

async function isUp(url: string): Promise<boolean> {
  try {
    const r = await fetch(url, { signal: AbortSignal.timeout(1500) });
    return r.ok;
  } catch {
    return false;
  }
}

async function waitUp(url: string, label: string, child?: ChildProcess, timeoutMs = 30_000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await isUp(url)) return;
    if (child && child.exitCode !== null) throw new Error(`${label} exited early with code ${child.exitCode}`);
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`${label} did not come up at ${url} within ${timeoutMs} ms`);
}

function start(label: string, args: string[], env: Record<string, string>): ChildProcess {
  const child = spawn(BUN, args, {
    cwd: REPO_ROOT,
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const verbose = process.env.RS_HARNESS_VERBOSE === "1";
  child.stdout?.on("data", (d) => verbose && process.stdout.write(`[${label}] ${d}`));
  child.stderr?.on("data", (d) => process.stderr.write(`[${label}] ${d}`));
  return child;
}

/** A mock server left running from before its source last changed would quietly serve old
 *  fixtures (it once survived a day), so reusing one is only allowed while it's current. */
async function refuseStaleMock() {
  const health = (await (await fetch(`${MOCK_URL}/__mock/health`)).json()) as { startedAt?: number };
  const dir = join(REPO_ROOT, "tests/mock-server");
  const changed = Math.max(...readdirSync(dir).filter((f) => f.endsWith(".ts")).map((f) => statSync(join(dir, f)).mtimeMs));
  if (!health.startedAt || health.startedAt < changed) {
    throw new Error(
      `The mock server already running at ${MOCK_URL} is older than tests/mock-server/. Stop it (lsof -nP -iTCP:${MOCK_PORT} -sTCP:LISTEN) or pick another MOCK_PORT.`,
    );
  }
}

export default async function globalSetup() {
  const children: ChildProcess[] = [];

  if (!(await isUp(`${MOCK_URL}/__mock/health`))) {
    const child = start("mock", ["tests/mock-server/server.ts"], { MOCK_PORT: String(MOCK_PORT), MOCK_QUIET: "1" });
    children.push(child);
    await waitUp(`${MOCK_URL}/__mock/health`, "mock server", child);
  } else {
    await refuseStaleMock();
  }

  if (!process.env.RS_WEB_URL && !(await isUp(WEB_URL))) {
    if (!existsSync(join(WEB_DIR, "index.html"))) {
      if (process.env.RS_BUILD === "1") {
        console.log("Building web release (dx build --platform web --release)...");
        const r = spawnSync("./scripts/run-with-rustup-env.sh", ["dx", "build", "--platform", "web", "--release"], {
          cwd: REPO_ROOT,
          stdio: "inherit",
        });
        if (r.status !== 0) throw new Error("dx build failed");
      } else {
        throw new Error(
          `No web build at ${WEB_DIR}. Run \`just web-build\` first, set RS_BUILD=1 to build automatically, or set RS_WEB_URL to a running server.`,
        );
      }
    }
    const child = start("web", ["tests/support/static-server.ts", WEB_DIR], { RS_WEB_PORT: String(WEB_PORT) });
    children.push(child);
    await waitUp(WEB_URL, "static web server", child);
  }

  return async () => {
    for (const c of children) c.kill("SIGTERM");
  };
}
