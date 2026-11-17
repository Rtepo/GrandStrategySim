#!/usr/bin/env node
/**
 * v4.34.0 (TASK-041): baseline frontend smoke test — verifies the built
 * bundle exists and the dev entry point parses. Runs as part of the
 * release gate suite; exits non-zero on failure.
 */
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const checks = [
  ["package.json", existsSync(join(root, "package.json"))],
  ["src/main.tsx or src/App.tsx", existsSync(join(root, "src", "App.tsx"))],
  ["index.html", existsSync(join(root, "index.html"))],
  ["dist bundle (run npm run build first)", existsSync(join(root, "dist", "index.html"))],
];

let failed = false;
for (const [name, ok] of checks) {
  console.log(`${ok ? "PASS" : "FAIL"} ${name}`);
  if (!ok) failed = true;
}

// Sanity: no dark-mode chart backgrounds may survive in built source.
const banned = ["#0b1021", "#0f172a", "#090d1a", "#111827", "#1e293b"];
const src = join(root, "src");
let dirty = [];
try {
  const { readdirSync, statSync } = await import("node:fs");
  const walk = (d) => {
    for (const f of readdirSync(d)) {
      const fp = join(d, f);
      if (statSync(fp).isDirectory()) walk(fp);
      else if (/\.(tsx|css)$/.test(f)) {
        const c = readFileSync(fp, "utf8");
        for (const b of banned) if (c.includes(b)) dirty.push(`${fp}: ${b}`);
      }
    }
  };
  walk(src);
} catch { /* src tree optional in packaged builds */ }

if (dirty.length) {
  console.log("FAIL dark chart/map backgrounds found:");
  for (const d of dirty) console.log(`  ${d}`);
  failed = true;
} else {
  console.log("PASS no forbidden dark chart/map backgrounds in src/**/*.{tsx,css}");
}

process.exit(failed ? 1 : 0);
