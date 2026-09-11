// Generated from crates/skema-wasm by scripts/build-model-package.mjs.
// Do not edit: these are skema-api's own structures, serialized by the same
// serde derives that produce the WASM and `skema/model` payloads.

/**
 * A half-open range in **UTF-16 code units** – directly usable as a
 * CodeMirror/Monaco document position pair.
 */
export interface Utf16Range {
    start: number;
    end: number;
}

/**
 * A named result type at the boundary, like [`crate::meta::TableMetas`] –
 * `tsify` implements the wasm ABI for a struct, not for a bare `Vec`.
 */
export interface StyleBlocks {
    blocks: StyleBlock[];
}

/**
 * A named result type at the boundary, like `CheckResult`/`ExportResult`
 * elsewhere in this crate – `tsify` implements the wasm ABI for a struct,
 * not for a bare `Vec` of one.
 */
export interface TableMetas {
    tables: TableMeta[];
}

/**
 * One `[]`-flag as written: `[pk]` carries `true`, `[default=0]` its
 * stringified value.
 *
 * A list of named entries rather than a map, deliberately. `model::Flags`
 * is a `BTreeMap`, and a Rust map crosses the wasm boundary as a JS
 * `Map` – while a bare `pub type` alias emits **no** TypeScript
 * declaration at all, so the generated `.d.ts` referred to a `Flags` type
 * it never defined and every consumer silently received `any`. A declared
 * struct is honest in both directions: the typings say exactly what
 * arrives, and what arrives is a plain array in the `BTreeMap`\'s order.
 */
export interface ExportedFlag {
    name: string;
    value: FlagValue;
}

/**
 * One object rather than three loose arguments: the file set is a
 * structured value, and `tsify` gives the TypeScript side a named type for
 * it instead of a positional argument list to keep straight.
 */
export interface ProjectInput {
    files: ProjectFile[];
    /**
     * The file carrying the one `Project` block (`docs/02`, \"that file is
     * the root file\"). Everything reachable from it via `import` forms the
     * project.
     */
    rootPath: string;
    withLocations: boolean;
}

/**
 * Physical names after the `Project` → `Document` → `Table` cascade. No
 * span: it\'s a cascade of up to three independent source blocks, so there
 * is no single correct source location for it – inventing one would be
 * actively misleading.
 */
export interface ExportedPhysical {
    database: string | undefined;
    schema: string | undefined;
    table: string | undefined;
}

/**
 * See [`TableMetas`].
 */
export interface EnumMetas {
    enums: EnumMeta[];
}

/**
 * See [`TableMetas`].
 */
export interface GroupMetas {
    groups: GroupMeta[];
}

/**
 * The two cascade levels from `docs/03`: a `Project`-level block is the
 * project-wide default, a `Document`-level one is that file\'s override.
 */
export type StyleScope = "project" | "document";

/**
 * Unlike the model entities below, a diagnostic always gets a `location`:
 * the list is always small, and a diagnostic without a human position is
 * nearly useless to an editor. `withLocations` only gates the (potentially
 * large) per-entity `location` field on the model.
 */
export interface Diagnostic {
    span: Span;
    location: Location;
    severity: Severity;
    message: string;
}

export interface Builtins {
    metaKeys: string[];
    lintKeys: string[];
    databaseKeys: string[];
    projectKeys: string[];
    /**
     * Keys inside a `Field { … }` description block (`docs/06`, `docs/08`).
     */
    fieldBlockKeys: string[];
    /**
     * Keys in a `Datatype` parameter\'s body.
     */
    paramBodyKeys: string[];
    /**
     * Keys in an `Enum` entry\'s body.
     */
    entryBodyKeys: string[];
    /**
     * Keys in a table column\'s `{ … }` body.
     */
    columnBodyKeys: string[];
    /**
     * Keys valid inside a field\'s `[ … ]` flags.
     */
    flagKeys: string[];
    indexFlagKeys: string[];
    builtinTypes: string[];
    /**
     * Every block keyword the parser recognizes, straight from
     * `BlockKind` – the same list that decides whether a word opens a
     * block at all (`grammar::parse_item_kind`).
     */
    blockKeywords: string[];
}

export interface CheckResult {
    ok: boolean;
    diagnostics: Diagnostic[];
}

export interface EnumMeta {
    name: string;
    /**
     * Right past the `Enum`\'s own `{` – see [`TableMeta::table_body_start`].
     */
    enumBodyStart: number;
    metaBodyStart: number | undefined;
    entries: MetaEntry[];
}

export interface ExportResult {
    ok: boolean;
    model: ExportedModel;
    diagnostics: Diagnostic[];
}

export interface ExportedEnum {
    file: string;
    namespace: string;
    name: string;
    storedType: string;
    entries: ExportedEnumEntry[];
    span: Span;
    location: Location | undefined;
}

export interface ExportedEnumEntry {
    name: string;
    label: string | undefined;
    value: string | undefined;
    span: Span;
    location: Location | undefined;
}

export interface ExportedField {
    /**
     * Display path of the file this field\'s own text lives in – **not
     * necessarily** the owning table\'s `file`: a field inlined via `use`
     * from a `Partial` declared in another (imported) file keeps that
     * file here, so `span`/`location` stay unambiguous.
     */
    file: string;
    name: string;
    ty: ExportedTypeRef;
    flags: ExportedFlag[];
    relation: ExportedRelation | undefined;
    note: string | undefined;
    span: Span;
    location: Location | undefined;
}

export interface ExportedGroup {
    /**
     * `Group` members are plain references, never inlined content, so –
     * unlike [`ExportedField`]/[`ExportedIndex`] – one file suffices for
     * the whole group.
     */
    file: string;
    name: string;
    members: ExportedGroupMember[];
    span: Span;
    location: Location | undefined;
}

export interface ExportedGroupMember {
    kind: string;
    namespace: string;
    name: string;
    span: Span;
    location: Location | undefined;
}

export interface ExportedIndex {
    /**
     * Same \"not necessarily the owning table\'s file\" caveat as
     * [`ExportedField::file`].
     */
    file: string;
    name: string | undefined;
    columns: string[];
    flags: ExportedFlag[];
    span: Span;
    location: Location | undefined;
}

export interface ExportedModel {
    tables: ExportedTable[];
    enums: ExportedEnum[];
    groups: ExportedGroup[];
}

export interface ExportedRelation {
    kind: RelationKind;
    /**
     * Namespace of the target table – with `targetTable`, the pair that
     * identifies it project-wide (a bare name can occur in several files).
     */
    targetNamespace: string;
    targetTable: string;
    targetField: string;
    onDelete: string | undefined;
    onUpdate: string | undefined;
    span: Span;
    location: Location | undefined;
}

export interface ExportedTable {
    /**
     * Display path of the file this `Table` is declared in – the file a
     * position-editing tool must open/edit to change this table\'s own
     * `Meta`, independent of which file(s) its fields come from.
     */
    file: string;
    namespace: string;
    name: string;
    physical: ExportedPhysical;
    fields: ExportedField[];
    indexes: ExportedIndex[];
    span: Span;
    location: Location | undefined;
}

export interface ExportedTypeRef {
    name: string;
    args: string[];
}

export interface GroupMeta {
    name: string;
    /**
     * Right past the `Group`\'s own `{` – see [`TableMeta::table_body_start`].
     */
    groupBodyStart: number;
    metaBodyStart: number | undefined;
    entries: MetaEntry[];
}

export interface Location {
    file: string;
    line: number;
    column: number;
    endLine: number;
    endColumn: number;
}

export interface MetaEntry {
    /**
     * The key as written, without the quotes a custom `\"x-…\"` key carries.
     */
    key: string;
    /**
     * The value\'s content (`ast::Value::to_text`) – a string without its
     * quotes, a raw expression without its backticks. `None` for a
     * valueless entry, which is a `[]`-flag shape and an error inside
     * `{}` – reported by `skema-hir`, not by this reader.
     */
    value: string | undefined;
    keyRange: Utf16Range;
    /**
     * The value **as written**, quotes included: replacing exactly this
     * range (with a freshly quoted string) changes the value and nothing
     * else – no reformatting, no touched neighbours.
     */
    valueRange: Utf16Range | undefined;
    /**
     * The whole `key = value` pair, without the terminating `;`.
     */
    entryRange: Utf16Range;
}

export interface ProjectFile {
    /**
     * A virtual path, `/`-separated. Only ever compared and joined as
     * text – there is no filesystem behind it.
     */
    path: string;
    source: string;
}

export interface ProjectFileStatus {
    path: string;
    /**
     * Reachable from the root file, and therefore part of the analysis
     * (`docs/02`, \"project membership is reachability-defined\"). A file
     * that is `false` here still gets its **syntax** checked – so a
     * not-yet-imported file being edited isn\'t silently unchecked – but no
     * cross-file rule is applied to it.
     */
    inProject: boolean;
}

export interface ProjectResult {
    ok: boolean;
    /**
     * `false` when `rootPath` names no file in `files` – a project-level
     * state with no source location to attach a diagnostic to, so it is
     * reported as its own flag rather than as a diagnostic pointing at a
     * file that isn\'t there.
     */
    rootFound: boolean;
    files: ProjectFileStatus[];
    model: ExportedModel;
    /**
     * Sorted by file, then position. `skema_hir` sorts by span alone,
     * which interleaves files by coincidental byte offset once more than
     * one is in play.
     */
    diagnostics: Diagnostic[];
}

export interface Span {
    start: number;
    end: number;
}

export interface StyleBlock {
    scope: StyleScope;
    /**
     * `Style @table { … }` → `Some(\"table\")`.
     */
    selector: string | undefined;
    /**
     * `Style highlight { … }` → `Some(\"highlight\")`. Exactly one of
     * `selector`/`class_name` is ever set.
     */
    className: string | undefined;
    entries: MetaEntry[];
}

export interface TableMeta {
    name: string;
    /**
     * Right past the `Table`\'s own `{` – where a whole new `Meta { … }`
     * block goes when the table has none yet.
     */
    tableBodyStart: number;
    /**
     * Right past the `Meta` block\'s `{`, when the table already has one –
     * where a new key goes into the existing block.
     */
    metaBodyStart: number | undefined;
    entries: MetaEntry[];
}

export type FlagValue = boolean | string;

export type RelationKind = "OneToMany" | "ManyToOne" | "OneToOne";

export type Severity = "error" | "warning" | "info" | "hint";

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly analyzeProject: (a: any) => any;
    readonly builtins: () => any;
    readonly check: (a: number, b: number, c: number, d: number) => any;
    readonly enumMetas: (a: number, b: number) => any;
    readonly exportModel: (a: number, b: number, c: number, d: number, e: number) => any;
    readonly groupMetas: (a: number, b: number) => any;
    readonly highlightClassNames: () => [number, number];
    readonly highlightTokens: (a: number, b: number) => [number, number];
    readonly init: () => void;
    readonly styleBlocks: (a: number, b: number) => any;
    readonly tableMetas: (a: number, b: number) => any;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
