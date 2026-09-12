#!/usr/bin/env node
// Builds the @skema/wasm package from the wasm-bindgen output.
//
//   (cd crates/skema-wasm && wasm-pack build --target web --release)
//   node scripts/build-model-package.mjs && node scripts/build-wasm-package.mjs
//
// This is the runtime half of the same generated file @skema/model carries the
// types of: the glue, the binary, and the functions. Its declarations *import*
// the model types rather than restating them, and the dependency on
// @skema/model is pinned exactly, so a consumer cannot end up holding a
// language version's wasm next to another version's types. That pairing used
// to be a convention between a committed binary and a semver range; here npm
// enforces it.
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { parse, workspaceVersion } from "./lib/wasm-dts.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source =
  process.env.SKEMA_WASM_DTS ?? join(root, "crates", "skema-wasm", "pkg", "skema_wasm.d.ts");
const sourceDir = dirname(source);
const packageDir = join(root, "npm", "wasm");

const RUNTIME_FILES = ["skema_wasm.js", "skema_wasm_bg.wasm", "skema_wasm_bg.wasm.d.ts"];

for (const file of [source, ...RUNTIME_FILES.map((name) => join(sourceDir, name))]) {
  if (existsSync(file)) continue;
  console.error(`No generated bindings at ${file}`);
  console.error("Build them: (cd crates/skema-wasm && wasm-pack build --target web --release)");
  process.exit(1);
}

const version = workspaceVersion(readFileSync(join(root, "Cargo.toml"), "utf8"));

if (!version) {
  console.error("Could not read [workspace.package] version from Cargo.toml");
  process.exit(1);
}

const { modelTypes, plumbingTypes, functions } = parse(readFileSync(source, "utf8"));

// Rewritten in place rather than beside the original: `.js` and `.d.ts` stay a
// matching pair, which is what wasm-pack's own `types` field expects.
const names = modelTypes.map((type) => type.name).sort();
const declarations = `// Generated from crates/skema-wasm by scripts/build-wasm-package.mjs.
// Do not edit. The model types come from @skema/model, which is generated from
// this same file - re-declaring them here would put two copies of skema-api's
// structures in one compilation.

import type {
${names.map((name) => `  ${name},`).join("\n")}
} from "@skema/model";

export type {
${names.map((name) => `  ${name},`).join("\n")}
};

${plumbingTypes.map((type) => type.text).join("\n\n")}

${functions.join("\n")}
`;

rmSync(packageDir, { recursive: true, force: true });
mkdirSync(packageDir, { recursive: true });

writeFileSync(join(packageDir, "skema_wasm.d.ts"), declarations);
for (const file of RUNTIME_FILES) {
  copyFileSync(join(sourceDir, file), join(packageDir, file));
}

// No `exports` field: consumers reach for `@skema/wasm/skema_wasm_bg.wasm?url`
// to let their bundler fingerprint the binary, which an export map would block.
writeFileSync(
  join(packageDir, "package.json"),
  `${JSON.stringify(
    {
      name: "@skema/wasm",
      version,
      description: "WASM bindings for skema: browser-facing check/export API.",
      license: "MIT OR Apache-2.0",
      repository: {
        type: "git",
        url: "https://gitlab.com/skema-language/skema.git",
        directory: "crates/skema-wasm",
      },
      type: "module",
      main: "skema_wasm.js",
      module: "skema_wasm.js",
      types: "skema_wasm.d.ts",
      files: ["skema_wasm.js", "skema_wasm.d.ts", "skema_wasm_bg.wasm", "skema_wasm_bg.wasm.d.ts"],
      sideEffects: false,
      dependencies: { "@skema/model": `=${version}` },
    },
    null,
    2,
  )}\n`,
);

console.log(
  `Wrote npm/wasm at ${version} (${names.length} types imported, ${functions.length} functions)`,
);
