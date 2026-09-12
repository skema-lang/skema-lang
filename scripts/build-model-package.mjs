#!/usr/bin/env node
// Regenerates the @skema/model package from the wasm-bindgen output.
//
//   (cd crates/skema-wasm && wasm-pack build --target web --release)
//   node scripts/build-model-package.mjs
//
// Only the model types come across. The generated file also declares the wasm
// entry points, which are a browser's way in: leaving them in an ambient
// declaration would let a value import compile and then fail at runtime. Its
// loader types go the same way — `InitOutput` carries `__wbindgen_malloc` and
// the rest of the module's internals, which say nothing about the model and
// have no meaning to a consumer that never instantiates the wasm.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { parse, workspaceVersion } from "./lib/wasm-dts.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source =
  process.env.SKEMA_WASM_DTS ?? join(root, "crates", "skema-wasm", "pkg", "skema_wasm.d.ts");
const packageDir = join(root, "npm", "model");

if (!existsSync(source)) {
  console.error(`No generated bindings at ${source}`);
  console.error("Build them: (cd crates/skema-wasm && wasm-pack build --target web --release)");
  process.exit(1);
}

const version = workspaceVersion(readFileSync(join(root, "Cargo.toml"), "utf8"));

if (!version) {
  console.error("Could not read [workspace.package] version from Cargo.toml");
  process.exit(1);
}

const { modelTypes } = parse(readFileSync(source, "utf8"));

const header = `// Generated from crates/skema-wasm by scripts/build-model-package.mjs.
// Do not edit: these are skema-api's own structures, serialized by the same
// serde derives that produce the WASM and \`skema/model\` payloads.

`;

writeFileSync(
  join(packageDir, "index.d.ts"),
  header + modelTypes.map((type) => type.text).join("\n\n") + "\n",
);

// The checked-in manifest is a template without a version: it is stamped here
// so that the number can only ever come from Cargo.toml.
const { name, ...rest } = JSON.parse(readFileSync(join(packageDir, "package.json"), "utf8"));
writeFileSync(
  join(packageDir, "package.json"),
  `${JSON.stringify({ name, version, ...rest }, null, 2)}\n`,
);

console.log(`Wrote npm/model at ${version} (${modelTypes.length} types)`);
