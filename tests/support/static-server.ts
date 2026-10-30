// Static file server for the `dx build --platform web --release` output, with SPA fallback
// so deep links like /albums or /album/:id/:server load index.html.
//
//   bun tests/support/static-server.ts [dir]     (port from RS_WEB_PORT, default 4580)
import { existsSync, statSync } from "node:fs";
import { join, normalize, resolve } from "node:path";

const root = resolve(process.argv[2] ?? process.env.RS_WEB_DIR ?? "target/dx/rustysound/release/web/public");
const port = Number(process.env.RS_WEB_PORT ?? 4580);
const host = process.env.RS_WEB_HOST ?? "127.0.0.1";

if (!existsSync(join(root, "index.html"))) {
  console.error(`static-server: no index.html in ${root}. Run: just web-build (dx build --platform web --release)`);
  process.exit(1);
}

Bun.serve({
  port,
  hostname: host,
  fetch(req) {
    const url = new URL(req.url);
    const rel = normalize(decodeURIComponent(url.pathname)).replace(/^(\.\.[/\\])+/, "");
    let path = join(root, rel);
    if (!path.startsWith(root)) return new Response("forbidden", { status: 403 });
    if (!existsSync(path) || statSync(path).isDirectory()) {
      // Asset-looking paths 404; route-looking paths get the SPA shell.
      if (/\.[a-z0-9]{1,8}$/i.test(rel)) return new Response("not found", { status: 404 });
      path = join(root, "index.html");
    }
    const file = Bun.file(path);
    const headers: Record<string, string> = {
      "Cache-Control": path.endsWith("index.html") ? "no-cache" : "public, max-age=31536000, immutable",
    };
    if (path.endsWith(".wasm")) headers["Content-Type"] = "application/wasm";
    return new Response(file, { headers });
  },
});

console.log(`static-server: ${root} on http://${host}:${port}`);
