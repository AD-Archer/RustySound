// Shared harness settings. Everything can be overridden from the environment.
import { resolve } from "node:path";

export const REPO_ROOT = resolve(import.meta.dirname, "../..");
export const MOCK_PORT = Number(process.env.MOCK_PORT ?? 4533);
export const MOCK_URL = process.env.RS_MOCK_URL ?? `http://127.0.0.1:${MOCK_PORT}`;
export const WEB_PORT = Number(process.env.RS_WEB_PORT ?? 4580);
/** Set RS_WEB_URL to test an already-running server (e.g. `dx serve`); no static server is started then. */
export const WEB_URL = process.env.RS_WEB_URL ?? `http://127.0.0.1:${WEB_PORT}`;
export const WEB_DIR = resolve(REPO_ROOT, process.env.RS_WEB_DIR ?? "target/dx/rustysound/release/web/public");
export const MOCK_USER = process.env.MOCK_USER ?? "demo";
export const MOCK_PASSWORD = process.env.MOCK_PASSWORD ?? "demo";
export const SCRATCH_OUT = process.env.RS_PERF_OUT ?? resolve(REPO_ROOT, "tests/.out/perf");
