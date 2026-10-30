// Summarise the axe sweep (tests/e2e/redesign/a11y.spec.ts) into markdown, by screen.
//
//   bun tests/support/axe-summary.ts [tests/.out/axe] [--out file.md]
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const args = process.argv.slice(2);
const dir = args[0] && !args[0].startsWith("--") ? args[0] : "tests/.out/axe";
const outIdx = args.indexOf("--out");
const out = outIdx >= 0 ? args[outIdx + 1] : null;

type Finding = { id: string; impact: string; help: string; nodes: number; targets: string[]; summary: string };
type Doc = { screen: string; scheme: string; project: string; compact: boolean; pageToken: string; findings: Finding[] };

if (!existsSync(dir)) {
  console.error(`no ${dir}`);
  process.exit(1);
}
const docs: Doc[] = [];
for (const project of readdirSync(dir)) {
  for (const f of readdirSync(join(dir, project))) if (f.endsWith(".json")) docs.push(JSON.parse(readFileSync(join(dir, project, f), "utf8")));
}
const projects = [...new Set(docs.map((d) => d.project))].sort();
const screens = [...new Set(docs.map((d) => d.screen))];
const count = (d: Doc | undefined) => {
  if (!d) return "–";
  if (!d.findings.length) return "0";
  const by = (i: string) => d.findings.filter((f) => f.impact === i).length;
  const tags: [string, string][] = [["critical", "C"], ["serious", "S"], ["moderate", "M"], ["minor", "m"]];
  return `${d.findings.length} (${tags.filter(([i]) => by(i)).map(([i, t]) => `${t}${by(i)}`).join(" ")})`;
};

let md = `# axe sweep\n\nCells: number of violated rules (C critical, S serious, M moderate, m minor). WCAG 2.0/2.1 A + AA tags.\n\n`;
const cols = projects.flatMap((p) => ["light", "dark"].map((s) => `${p} ${s}`));
md += `| Screen | ${cols.join(" | ")} |\n|---|${cols.map(() => "---").join("|")}|\n`;
for (const s of screens) {
  md += `| ${s} | ${projects
    .flatMap((p) => ["light", "dark"].map((sc) => count(docs.find((d) => d.screen === s && d.project === p && d.scheme === sc))))
    .join(" | ")} |\n`;
}
md += `\n## Findings by screen\n`;
for (const s of screens) {
  const ds = docs.filter((d) => d.screen === s && d.findings.length);
  if (!ds.length) continue;
  md += `\n### ${s}\n`;
  const byRule = new Map<string, { f: Finding; where: string[] }>();
  for (const d of ds) {
    for (const f of d.findings) {
      const e = byRule.get(f.id) ?? { f, where: [] };
      e.where.push(`${d.project} ${d.scheme} (${f.nodes})`);
      byRule.set(f.id, e);
    }
  }
  for (const { f, where } of byRule.values()) {
    md += `- **${f.impact} ${f.id}**: ${f.help}. Where (nodes): ${where.join(", ")}. e.g. \`${f.targets[0]}\`${f.summary ? `: ${f.summary}` : ""}\n`;
  }
}
if (out) writeFileSync(out, md);
console.log(md);
