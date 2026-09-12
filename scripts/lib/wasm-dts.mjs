// Splits the wasm-bindgen `.d.ts` into the three groups the two npm packages
// need, so both generators agree on what a "model type" is by construction
// rather than by two similar regexes drifting apart.
//
// The declarations arrive in one file but have two different authors: tsify
// emits skema-api's DTOs, while wasm-bindgen adds its own loader plumbing.
// Only the first group is the model, and only it belongs in @skema/model.

/** wasm-bindgen's own loader types - they describe the module, not the model. */
const PLUMBING = new Set(["InitInput", "InitOutput", "SyncInitInput"]);

/**
 * @param {string} source contents of `crates/skema-wasm/pkg/skema_wasm.d.ts`
 * @returns {{ modelTypes: {name: string, text: string}[],
 *             plumbingTypes: {name: string, text: string}[],
 *             functions: string[] }}
 */
export function parse(source) {
  const lines = source.split("\n");
  const modelTypes = [];
  const plumbingTypes = [];
  const functions = [];
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
      const name = line.slice("export interface ".length).split(/[\s{]/)[0];
      const body = [...comment, line];
      while (lines[i] !== "}" && i + 1 < lines.length) {
        i += 1;
        body.push(lines[i]);
      }
      (PLUMBING.has(name) ? plumbingTypes : modelTypes).push({
        name,
        text: body.join("\n"),
      });
      comment = [];
      continue;
    }

    if (line.startsWith("export type ")) {
      const name = line.slice("export type ".length).split(/[\s=]/)[0];
      (PLUMBING.has(name) ? plumbingTypes : modelTypes).push({
        name,
        text: [...comment, line].join("\n"),
      });
      comment = [];
      continue;
    }

    // `export default function __wbg_init` included: it is the loader entry
    // point, and a package that dropped it could not be initialised at all.
    if (line.startsWith("export function ") || line.startsWith("export default function ")) {
      functions.push([...comment, line].join("\n"));
      comment = [];
      continue;
    }

    if (line.trim() !== "") comment = [];
  }

  return { modelTypes, plumbingTypes, functions };
}

/** Reads `[workspace.package] version` - the one version every artifact carries. */
export function workspaceVersion(cargoToml) {
  return cargoToml.split("[workspace.package]")[1]?.match(/^version = "([^"]+)"/m)?.[1];
}
