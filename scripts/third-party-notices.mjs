#!/usr/bin/env node
// Generates THIRD_PARTY_NOTICES.md: the licence texts of everything compiled
// into the Open Herd executable. That is the Rust crates (from cargo-about) and
// the npm packages Vite bundles into the frontend.
//
//   node scripts/third-party-notices.mjs           # rewrite the file
//   node scripts/third-party-notices.mjs --check   # exit 1 if it is stale (CI)
//
// Needs `cargo about` (cargo install --locked cargo-about) and `npm ci` done.
// The crate list and accepted licences are in src-tauri/about.toml. The policy
// that blocks GPL-incompatible licences is in src-tauri/deny.toml.

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outFile = join(root, "THIRD_PARTY_NOTICES.md");
const check = process.argv.includes("--check");

// Normalise a licence text so identical texts group together whatever their
// line endings or trailing whitespace.
const clean = (text) =>
  text.replace(/\r\n?/g, "\n").replace(/[ \t]+$/gm, "").trim();

// ── Rust crates (cargo-about) ───────────────────────────────────────────────

function rustLicenses() {
  // -o, not stdout: cargo-about refuses to write to a pipe under PowerShell.
  const tmp = mkdtempSync(join(tmpdir(), "open-herd-about-"));
  const jsonFile = join(tmp, "about.json");
  let about;
  try {
    execFileSync(
      "cargo",
      // --frozen: Cargo.lock as committed and no network, so the output only
      // depends on the lockfile (CI runs `cargo fetch --locked` first).
      ["about", "generate", "--format", "json", "--frozen", "--fail", "-o", jsonFile],
      { cwd: join(root, "src-tauri"), stdio: ["ignore", "inherit", "inherit"] },
    );
    about = JSON.parse(readFileSync(jsonFile, "utf8"));
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  const groups = new Map(); // text -> { id, users:Set }
  for (const lic of about.licenses) {
    const text = clean(lic.text);
    const users = lic.used_by
      .map((u) => `${u.crate.name} ${u.crate.version}`)
      .filter((u) => !u.startsWith("open-herd "));
    if (users.length === 0) continue;
    const g = groups.get(text) ?? { id: lic.id, users: new Set() };
    users.forEach((u) => g.users.add(u));
    groups.set(text, g);
  }
  return groups;
}

// ── npm packages bundled into the frontend ──────────────────────────────────

const repoUrl = (meta) => {
  const url = typeof meta.repository === "string" ? meta.repository : meta.repository?.url;
  return url?.replace(/^git\+/, "").replace(/\.git$/, "");
};

const LICENSE_FILE =/^(licen[cs]e|copying|notice)(\.(md|txt|markdown))?$/i;

function npmLicenses() {
  const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8"));
  const groups = new Map();
  for (const [path, pkg] of Object.entries(lock.packages)) {
    // "" is open-herd itself; dev-only packages are never shipped.
    if (path === "" || pkg.dev || pkg.devOptional) continue;
    const dir = join(root, path);
    if (!existsSync(dir)) continue; // optional package for another platform
    const meta = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const id = pkg.license ?? meta.license ?? "UNKNOWN";
    const files = readdirSync(dir).filter((f) => LICENSE_FILE.test(f)).sort();
    const text = files.length
      ? files.map((f) => clean(readFileSync(join(dir, f), "utf8"))).join("\n\n")
      : `${meta.name} is licensed under ${id}. The package ships no license file; ` +
        `see ${repoUrl(meta) ?? "its npm page"} for the full text and copyright holders.`;
    const g = groups.get(text) ?? { id, users: new Set() };
    g.users.add(`${meta.name} ${meta.version}`);
    groups.set(text, g);
  }
  return groups;
}

// ── Markdown ────────────────────────────────────────────────────────────────

const byName = (a, b) => a.localeCompare(b, "en");

function section(title, groups) {
  const entries = [...groups.entries()]
    .map(([text, g]) => ({ text, id: g.id, users: [...g.users].sort(byName) }))
    .sort((a, b) => byName(a.id, b.id) || byName(a.users[0], b.users[0]));
  const count = new Set(entries.flatMap((e) => e.users)).size;

  const totals = new Map();
  for (const e of entries) totals.set(e.id, (totals.get(e.id) ?? 0) + e.users.length);
  const summary = [...totals.entries()]
    .sort((a, b) => b[1] - a[1] || byName(a[0], b[0]))
    .map(([id, n]) => `| ${id} | ${n} |`)
    .join("\n");

  const bodies = entries
    .map(
      (e) =>
        `### ${e.id}\n\nUsed by: ${e.users.join(", ")}\n\n~~~~text\n${e.text}\n~~~~`,
    )
    .join("\n\n");

  return `## ${title}\n\n${count} packages.\n\n| License | Packages |\n|---|---|\n${summary}\n\n${bodies}`;
}

const header = `# Third-party notices

Open Herd is licensed under the GNU General Public License, version 3 (see
\`LICENSE\`). Its source code, for every release, is at
https://github.com/eder-rimarachinr/open_herd.

The Open Herd executable includes the open-source components listed below.
Each one is distributed under its own license, reproduced here as its authors
require. This file is generated by \`scripts/third-party-notices.mjs\`: do not
edit it by hand.

## Downloaded at runtime, not included

The installer does not contain these programs. Open Herd downloads them from
their official sources, at your request, onto your computer. Their own
licenses apply:

- nginx (https://nginx.org/LICENSE), BSD-2-Clause
- PHP for Windows (https://windows.php.net) and static-php-cli builds on Linux
  (https://static-php.dev), under the PHP license and the licenses of the
  libraries compiled into them (https://www.php.net/license/)
- mkcert (https://github.com/FiloSottile/mkcert), BSD-3-Clause
- Node.js (https://github.com/nodejs/node/blob/main/LICENSE), MIT and the
  licenses of its bundled dependencies
- Composer (https://github.com/composer/composer), MIT
- Microsoft Edge WebView2 Runtime, fetched by the Windows installer if missing,
  under Microsoft's license terms`;

const output = `${header}\n\n${section("Rust crates", rustLicenses())}\n\n${section(
  "JavaScript packages (frontend)",
  npmLicenses(),
)}\n`;

if (check) {
  const current = existsSync(outFile) ? clean(readFileSync(outFile, "utf8")) : "";
  if (current !== clean(output)) {
    console.error(
      "THIRD_PARTY_NOTICES.md is stale. Run 'node scripts/third-party-notices.mjs' and commit it.",
    );
    process.exit(1);
  }
  console.log("THIRD_PARTY_NOTICES.md is up to date.");
} else {
  writeFileSync(outFile, output);
  console.log(`Wrote ${outFile}`);
}
