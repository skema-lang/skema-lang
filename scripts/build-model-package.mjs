#!/usr/bin/env node
// Regenerates the @skema-language/model package from the wasm-bindgen output.
//
//   (cd crates/skema-wasm && wasm-pack build --target web --release)
//   node scripts/build-model-package.mjs
//
// Only the types come across. The generated file also declares the wasm entry
// points, which are a browser's way in: leaving them in an ambient declaration
// would let a value import compile and then fail at runtime.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source =
  process.env.SKEMA_WASM_DTS ?? join(root, "crates", "skema-wasm", "pkg", "skema_wasm.d.ts");
const packageDir = join(root, "npm", "model");

if (!existsSync(source)) {
  console.error(`No generated bindings at ${source}`);
  console.error("Build them: (cd crates/skema-wasm && wasm-pack build --target web --release)");
  process.exit(1);
}

const version = readFileSync(join(root, "Cargo.toml"), "utf8")
  .split("[workspace.package]")[1]
  ?.match(/^version = "([^"]+)"/m)?.[1];

if (!version) {
  console.error("Could not read [workspace.package] version from Cargo.toml");
  process.exit(1);
}

const lines = readFileSync(source, "utf8").split("\n");
const kept = [];
let comment = [];

for (let i = 0; i < lines.length; i += 1) {
  const line = lines[i];

  if (line.startsWith("/**")) {
    comment = [line];
    while (!lines[i].includes("*/") && i + 1 < lines.length) {
      i += 1;
      comment.push(lines[i]);
    }
    continue;
  }

  if (line.startsWith("export interface ")) {
    kept.push(...comment, line);
    while (lines[i] !== "}" && i + 1 < lines.length) {
      i += 1;
      kept.push(lines[i]);
    }
    kept.push("");
    comment = [];
    continue;
  }

  if (line.startsWith("export type ")) {
    kept.push(...comment, line, "");
    comment = [];
    continue;
  }

  if (line.trim() !== "") comment = [];
}

const header = `// Generated from crates/skema-wasm by scripts/build-model-package.mjs.
// Do not edit: these are skema-api's own structures, serialized by the same
// serde derives that produce the WASM and \`skema/model\` payloads.

`;

writeFileSync(
  join(packageDir, "index.d.ts"),
  header + kept.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n",
);

const manifest = JSON.parse(readFileSync(join(packageDir, "package.json"), "utf8"));
manifest.version = version;
writeFileSync(join(packageDir, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);

console.log(`Wrote npm/model at ${version} (${kept.length} lines of declarations)`);
