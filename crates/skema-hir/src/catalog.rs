//! Public, stable catalogs of the language's own vocabulary – known key
//! names, flag names, built-in type names.
//!
//! `docs/01`, "Custom keys": `rules::keys` and `rules::resolve` check
//! *against* these lists. Public so the completion in `skema-wasm` and
//! `skema-lsp` can read the same catalog instead of duplicating it.

use TypeParamSlot::{Int, OptInt, OptOneOf};

pub const META_KEYS: &[&str] = &["author", "note", "version", "styleClasses", "movedFrom"];

/// The subset of [`META_KEYS`] legal only in a `Table`'s `Meta`
/// (`docs/03`) – `rules::keys` rejects them elsewhere with the same
/// diagnostic an unknown key gets.
pub const TABLE_ONLY_META_KEYS: &[&str] = &["movedFrom"];
pub const LINT_KEYS: &[&str] = &[
    "fieldNaming",
    "tableNaming",
    "enumNaming",
    "unusedImport",
    "danglingRef",
    "requirePrimaryKey",
    "requireForeignKeyIndex",
    "requireTableNote",
    "requireFieldNote",
];
/// The subset of [`LINT_KEYS`] whose value slot is a case style rather than
/// a severity – the only keys a `[warn|error]` override (`docs/04`) can
/// legally follow. `rules::keys` rejects the bracket everywhere else.
pub const SEVERITY_OVERRIDABLE_LINT_KEYS: &[&str] = &["fieldNaming", "tableNaming", "enumNaming"];
pub const DATABASE_KEYS: &[&str] = &["database", "schema", "table"];
pub const PROJECT_KEYS: &[&str] = &["mode", "dialect"];
pub const FIELD_BLOCK_KEYS: &[&str] = &["type", "default", "regex", "note"];
pub const PARAM_BODY_KEYS: &[&str] = &["note", "default"];
pub const ENTRY_BODY_KEYS: &[&str] = &["label", "value"];
pub const COLUMN_BODY_KEYS: &[&str] = &["note", "exampleValue", "label"];
pub const FLAG_KEYS: &[&str] = &[
    "pk", "unique", "required", "optional", "default", "column", "onDelete", "onUpdate",
];
pub const INDEX_FLAG_KEYS: &[&str] = &["unique"];

/// Values accepted by `onDelete`/`onUpdate` (`docs/13`, "Flag reference").
/// Enforced in `rules::resolve::check_relation_action`.
pub const RELATION_ACTION_VALUES: &[&str] =
    &["cascade", "restrict", "setNull", "setDefault", "noAction"];

/// Known `Dialect @engine` names (`docs/06`, "Dialect @engine blocks";
/// `docs/11`, "Engine-specific settings"). Also the set `Project.dialect`
/// accepts, minus `generic` – that value selects no engine at all.
pub const DIALECT_ENGINES: &[&str] = &["postgres", "mysql", "oracledb"];

/// The built-in, dialect-independent types (`docs/02`, `docs/06`).
///
/// Includes both [`DRAFT_TYPES`] and the precise types beyond it. This is
/// the complete dialect-independent catalog (task 17) – a project wanting
/// more precise built-ins reaches for a specific `dialect`'s own catalog
/// ([`POSTGRES_TYPES`], [`MYSQL_TYPES`], [`ORACLEDB_TYPES`]) or a
/// `Datatype`.
pub const BUILTIN_TYPES: &[&str] = &[
    "string",
    "int",
    "float",
    "bool",
    "varchar",
    "text",
    "uuid",
    "date",
    "timestamp",
];

/// The four types `mode = draft` gets by on (`docs/02`, "The mode switch") –
/// a subset of [`BUILTIN_TYPES`]. `draft` merely doesn't *require* more than
/// these; it still accepts every other built-in, an `Enum`, or a
/// `Datatype`. `strict` requires one of those more precise alternatives –
/// enforced in `rules::resolve::resolve_named_type`.
pub const DRAFT_TYPES: &[&str] = &["string", "int", "float", "bool"];

/// One argument slot of a parameterized type, in declaration order. A slot
/// after an optional one is optional too in every table below – the tables
/// don't re-state that, `rules::resolve::check_type_args` relies on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeParamSlot {
    /// A required `int` argument.
    Int,
    /// An `int` argument that may be omitted.
    OptInt,
    /// A bare-word argument that may be omitted, one of a closed set
    /// (`vector`'s `format`).
    OptOneOf(&'static [&'static str]),
}

/// The argument shape of a built-in type (`docs/06`, "Built-in types" /
/// "Parameters").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeParams {
    /// Takes no arguments.
    None,
    /// A fixed run of slots, read left to right.
    Slots(&'static [TypeParamSlot]),
    /// One or more string-literal arguments, e.g. `enum('a', 'b')`.
    StringList,
}

/// Parameter shape of every parameterized built-in in [`BUILTIN_TYPES`]
/// (`docs/06`, "Built-in types"): `varchar(n)` takes one length. Anything in
/// `BUILTIN_TYPES` not listed here takes no arguments at all.
pub const BUILTIN_TYPE_PARAMS: &[(&str, TypeParams)] =
    &[("varchar", TypeParams::Slots(&[TypeParamSlot::Int]))];

/// One entry of a [`dialect_type`] catalog: the accepted spelling(s) and
/// argument shape of one engine-specific built-in type.
#[derive(Debug, Clone, Copy)]
pub struct DialectType {
    pub name: &'static str,
    /// A second accepted spelling for the same type (e.g. `bigint`/`int8`) –
    /// both resolve to the same entry, same argument shape.
    pub alias: Option<&'static str>,
    pub params: TypeParams,
}

const fn ty(name: &'static str) -> DialectType {
    DialectType {
        name,
        alias: None,
        params: TypeParams::None,
    }
}

const fn ty_alias(name: &'static str, alias: &'static str) -> DialectType {
    DialectType {
        name,
        alias: Some(alias),
        params: TypeParams::None,
    }
}

const fn ty_params(name: &'static str, params: TypeParams) -> DialectType {
    DialectType {
        name,
        alias: None,
        params,
    }
}

const fn ty_alias_params(
    name: &'static str,
    alias: &'static str,
    params: TypeParams,
) -> DialectType {
    DialectType {
        name,
        alias: Some(alias),
        params,
    }
}

/// The full built-in type catalog for `dialect = postgres` (`docs/06`), on
/// top of the dialect-independent [`BUILTIN_TYPES`]. Sourced from
/// PostgreSQL's own "Data Types" reference (task 17,
/// `decisions.md#per-dialect-built-in-type-catalog-task-17`); excludes
/// `CREATE TYPE`-defined mechanisms (enums/composites/domains/ranges,
/// `type[]` arrays) since `Enum`/`Datatype` already cover that need.
pub const POSTGRES_TYPES: &[DialectType] = &[
    ty_alias("bigint", "int8"),
    ty_alias("smallint", "int2"),
    ty_alias("bigserial", "serial8"),
    ty_alias("serial", "serial4"),
    ty_alias("smallserial", "serial2"),
    ty_alias_params("numeric", "decimal", TypeParams::Slots(&[Int, OptInt])),
    ty_alias("real", "float4"),
    ty_alias("doublePrecision", "float8"),
    ty("money"),
    ty_alias_params("char", "character", TypeParams::Slots(&[Int])),
    ty("bytea"),
    ty_params("time", TypeParams::Slots(&[OptInt])),
    ty_alias_params("timeWithTimeZone", "timetz", TypeParams::Slots(&[OptInt])),
    ty_alias_params(
        "timestampWithTimeZone",
        "timestamptz",
        TypeParams::Slots(&[OptInt]),
    ),
    ty("interval"),
    ty("point"),
    ty("line"),
    ty("lseg"),
    ty("box"),
    ty("path"),
    ty("polygon"),
    ty("circle"),
    ty("inet"),
    ty("cidr"),
    ty("macaddr"),
    ty("macaddr8"),
    ty_params("bit", TypeParams::Slots(&[Int])),
    ty_alias_params("bitVarying", "varbit", TypeParams::Slots(&[Int])),
    ty("tsvector"),
    ty("tsquery"),
    ty("xml"),
    ty("jsonb"),
    ty("jsonpath"),
    ty("int4range"),
    ty("int8range"),
    ty("numrange"),
    ty("tsrange"),
    ty("tstzrange"),
    ty("daterange"),
    ty("oid"),
    ty("pg_lsn"),
    ty("pg_snapshot"),
];

/// The full built-in type catalog for `dialect = mysql` (`docs/06`). Sourced
/// from the MySQL 8.4 Reference Manual (task 17, same decision record as
/// [`POSTGRES_TYPES`]).
pub const MYSQL_TYPES: &[DialectType] = &[
    ty_params("tinyint", TypeParams::Slots(&[OptInt])),
    ty_params("smallint", TypeParams::Slots(&[OptInt])),
    ty_params("mediumint", TypeParams::Slots(&[OptInt])),
    ty_params("bigint", TypeParams::Slots(&[OptInt])),
    ty_alias_params("decimal", "numeric", TypeParams::Slots(&[Int, OptInt])),
    ty_params("double", TypeParams::Slots(&[OptInt, OptInt])),
    ty_params("bit", TypeParams::Slots(&[Int])),
    ty_params("time", TypeParams::Slots(&[OptInt])),
    ty_params("datetime", TypeParams::Slots(&[OptInt])),
    ty("year"),
    ty_params("char", TypeParams::Slots(&[Int])),
    ty_params("binary", TypeParams::Slots(&[Int])),
    ty_params("varbinary", TypeParams::Slots(&[Int])),
    ty("tinyblob"),
    ty("blob"),
    ty("mediumblob"),
    ty("longblob"),
    ty("tinytext"),
    ty("mediumtext"),
    ty("longtext"),
    ty_params("enum", TypeParams::StringList),
    ty_params("set", TypeParams::StringList),
    ty("geometry"),
    ty("point"),
    ty("linestring"),
    ty("polygon"),
    ty("multipoint"),
    ty("multilinestring"),
    ty("multipolygon"),
    ty("multicurve"),
    ty("multisurface"),
    ty("geometrycollection"),
    ty("json"),
];

/// The full built-in type catalog for `dialect = oracledb` (`docs/06`).
/// Sourced from the Oracle Database 23 SQL Language Reference (task 17,
/// same decision record as [`POSTGRES_TYPES`]). `timestamp` here overrides
/// the dialect-independent zero-argument entry with this dialect's own
/// optional fractional-seconds precision.
pub const ORACLEDB_TYPES: &[DialectType] = &[
    ty_params("char", TypeParams::Slots(&[Int])),
    ty_params("nchar", TypeParams::Slots(&[OptInt])),
    ty_params("varchar2", TypeParams::Slots(&[Int])),
    ty_params("nvarchar2", TypeParams::Slots(&[Int])),
    ty_params("number", TypeParams::Slots(&[OptInt, OptInt])),
    ty_params("float", TypeParams::Slots(&[OptInt])),
    ty("binary_float"),
    ty("binary_double"),
    ty_params("timestamp", TypeParams::Slots(&[OptInt])),
    ty_params("timestampWithTimeZone", TypeParams::Slots(&[OptInt])),
    ty_params("timestampWithLocalTimeZone", TypeParams::Slots(&[OptInt])),
    ty_params("intervalYearToMonth", TypeParams::Slots(&[OptInt])),
    ty_params("intervalDayToSecond", TypeParams::Slots(&[OptInt, OptInt])),
    ty_params("raw", TypeParams::Slots(&[Int])),
    ty("longRaw"),
    ty("rowid"),
    ty_params("urowid", TypeParams::Slots(&[OptInt])),
    ty("clob"),
    ty("nclob"),
    ty("blob"),
    ty("bfile"),
    ty("json"),
    ty_params(
        "vector",
        TypeParams::Slots(&[OptInt, OptOneOf(&["int8", "float32", "float64", "binary"])]),
    ),
    ty("long"),
];

/// The full catalog for `engine` (`"postgres"`/`"mysql"`/`"oracledb"`,
/// matching [`DIALECT_ENGINES`]) – empty for an unknown engine.
pub fn dialect_types(engine: &str) -> &'static [DialectType] {
    match engine {
        "postgres" => POSTGRES_TYPES,
        "mysql" => MYSQL_TYPES,
        "oracledb" => ORACLEDB_TYPES,
        _ => &[],
    }
}

/// Looks up one type name (or alias) in the catalog for `engine` – `None`
/// for an unknown engine or a name absent from that engine's catalog.
pub fn dialect_type(engine: &str, name: &str) -> Option<&'static DialectType> {
    dialect_types(engine)
        .iter()
        .find(|t| t.name == name || t.alias == Some(name))
}

// ===========================================================================
// Reserved words & identifier length limits (task 82)
// ===========================================================================

/// The full PostgreSQL reserved-word list (`docs/11`, "Reserved words and
/// identifier length"): every entry whose "PostgreSQL" column reads exactly
/// `reserved` (including the ", requires AS" variants – that annotation is
/// about table-alias syntax, not a different reservedness level) in the
/// current "SQL Key Words" appendix – excludes `non-reserved` and
/// `reserved (can be function or type)`, which may still be used as a
/// function or type name. Source: PostgreSQL 18,
/// <https://www.postgresql.org/docs/current/sql-keywords-appendix.html>
/// (fetched 2026-09-16).
pub const POSTGRES_RESERVED_WORDS: &[&str] = &[
    "ALL",
    "ANALYSE",
    "ANALYZE",
    "AND",
    "ANY",
    "ARRAY",
    "AS",
    "ASC",
    "ASYMMETRIC",
    "BOTH",
    "CASE",
    "CAST",
    "CHECK",
    "COLLATE",
    "COLUMN",
    "CONSTRAINT",
    "CREATE",
    "CURRENT_CATALOG",
    "CURRENT_DATE",
    "CURRENT_ROLE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "CURRENT_USER",
    "DEFAULT",
    "DEFERRABLE",
    "DESC",
    "DISTINCT",
    "DO",
    "ELSE",
    "END",
    "EXCEPT",
    "FALSE",
    "FETCH",
    "FOR",
    "FOREIGN",
    "FROM",
    "GRANT",
    "GROUP",
    "HAVING",
    "IN",
    "INITIALLY",
    "INTERSECT",
    "INTO",
    "LATERAL",
    "LEADING",
    "LIMIT",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "NOT",
    "NULL",
    "OFFSET",
    "ON",
    "ONLY",
    "OR",
    "ORDER",
    "PLACING",
    "PRIMARY",
    "REFERENCES",
    "RETURNING",
    "SELECT",
    "SESSION_USER",
    "SOME",
    "SYMMETRIC",
    "SYSTEM_USER",
    "TABLE",
    "THEN",
    "TO",
    "TRAILING",
    "TRUE",
    "UNION",
    "UNIQUE",
    "USER",
    "USING",
    "VARIADIC",
    "WHEN",
    "WHERE",
    "WINDOW",
    "WITH",
];

/// The full MySQL reserved-word list (`docs/11`): every keyword marked
/// `(R)` in the current Reference Manual. Source: MySQL 8.4,
/// <https://dev.mysql.com/doc/refman/8.4/en/keywords.html> (fetched
/// 2026-09-16).
pub const MYSQL_RESERVED_WORDS: &[&str] = &[
    "ACCESSIBLE",
    "ADD",
    "ALL",
    "ALTER",
    "ANALYZE",
    "AND",
    "AS",
    "ASC",
    "ASENSITIVE",
    "BEFORE",
    "BETWEEN",
    "BIGINT",
    "BINARY",
    "BLOB",
    "BOTH",
    "BY",
    "CALL",
    "CASCADE",
    "CASE",
    "CHANGE",
    "CHAR",
    "CHARACTER",
    "CHECK",
    "COLLATE",
    "COLUMN",
    "CONDITION",
    "CONSTRAINT",
    "CONTINUE",
    "CONVERT",
    "CREATE",
    "CROSS",
    "CUBE",
    "CUME_DIST",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "CURRENT_USER",
    "CURSOR",
    "DATABASE",
    "DATABASES",
    "DAY_HOUR",
    "DAY_MICROSECOND",
    "DAY_MINUTE",
    "DAY_SECOND",
    "DEC",
    "DECIMAL",
    "DECLARE",
    "DEFAULT",
    "DELAYED",
    "DELETE",
    "DENSE_RANK",
    "DESC",
    "DESCRIBE",
    "DETERMINISTIC",
    "DISTINCT",
    "DISTINCTROW",
    "DIV",
    "DOUBLE",
    "DROP",
    "DUAL",
    "EACH",
    "ELSE",
    "ELSEIF",
    "EMPTY",
    "ENCLOSED",
    "ESCAPED",
    "EXCEPT",
    "EXISTS",
    "EXIT",
    "EXPLAIN",
    "FALSE",
    "FETCH",
    "FIRST_VALUE",
    "FLOAT",
    "FLOAT4",
    "FLOAT8",
    "FOR",
    "FORCE",
    "FOREIGN",
    "FROM",
    "FULLTEXT",
    "FUNCTION",
    "GENERATED",
    "GET",
    "GRANT",
    "GROUP",
    "GROUPING",
    "GROUPS",
    "HAVING",
    "HIGH_PRIORITY",
    "HOUR_MICROSECOND",
    "HOUR_MINUTE",
    "HOUR_SECOND",
    "IF",
    "IGNORE",
    "IN",
    "INDEX",
    "INFILE",
    "INNER",
    "INOUT",
    "INSENSITIVE",
    "INSERT",
    "INT",
    "INT1",
    "INT2",
    "INT3",
    "INT4",
    "INT8",
    "INTEGER",
    "INTERSECT",
    "INTERVAL",
    "INTO",
    "IO_AFTER_GTIDS",
    "IO_BEFORE_GTIDS",
    "IS",
    "ITERATE",
    "JOIN",
    "JSON_TABLE",
    "KEY",
    "KEYS",
    "KILL",
    "LAG",
    "LAST_VALUE",
    "LATERAL",
    "LEAD",
    "LEADING",
    "LEAVE",
    "LEFT",
    "LIKE",
    "LIMIT",
    "LINEAR",
    "LINES",
    "LOAD",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "LOCK",
    "LONG",
    "LONGBLOB",
    "LONGTEXT",
    "LOOP",
    "LOW_PRIORITY",
    "MASTER_BIND",
    "MASTER_SSL_VERIFY_SERVER_CERT",
    "MATCH",
    "MAXVALUE",
    "MEDIUMBLOB",
    "MEDIUMINT",
    "MEDIUMTEXT",
    "MIDDLEINT",
    "MINUTE_MICROSECOND",
    "MINUTE_SECOND",
    "MOD",
    "MODIFIES",
    "NATURAL",
    "NOT",
    "NO_WRITE_TO_BINLOG",
    "NTH_VALUE",
    "NTILE",
    "NULL",
    "NUMERIC",
    "OF",
    "ON",
    "OPTIMIZE",
    "OPTIMIZER_COSTS",
    "OPTION",
    "OPTIONALLY",
    "OR",
    "ORDER",
    "OUT",
    "OUTER",
    "OUTFILE",
    "OVER",
    "PARTITION",
    "PERCENT_RANK",
    "PRECISION",
    "PRIMARY",
    "PROCEDURE",
    "PURGE",
    "QUALIFY",
    "RANGE",
    "RANK",
    "READ",
    "READS",
    "READ_WRITE",
    "REAL",
    "RECURSIVE",
    "REFERENCES",
    "REGEXP",
    "RELEASE",
    "RENAME",
    "REPEAT",
    "REPLACE",
    "REQUIRE",
    "RESIGNAL",
    "RESTRICT",
    "RETURN",
    "REVOKE",
    "RIGHT",
    "RLIKE",
    "ROW",
    "ROWS",
    "ROW_NUMBER",
    "SCHEMA",
    "SCHEMAS",
    "SECOND_MICROSECOND",
    "SELECT",
    "SENSITIVE",
    "SEPARATOR",
    "SET",
    "SHOW",
    "SIGNAL",
    "SMALLINT",
    "SPATIAL",
    "SPECIFIC",
    "SQL",
    "SQLEXCEPTION",
    "SQLSTATE",
    "SQLWARNING",
    "SQL_BIG_RESULT",
    "SQL_CALC_FOUND_ROWS",
    "SQL_SMALL_RESULT",
    "SSL",
    "STARTING",
    "STORED",
    "STRAIGHT_JOIN",
    "SYSTEM",
    "TABLE",
    "TABLESAMPLE",
    "TERMINATED",
    "THEN",
    "TINYBLOB",
    "TINYINT",
    "TINYTEXT",
    "TO",
    "TRAILING",
    "TRIGGER",
    "TRUE",
    "UNDO",
    "UNION",
    "UNIQUE",
    "UNLOCK",
    "UNSIGNED",
    "UPDATE",
    "USAGE",
    "USE",
    "USING",
    "UTC_DATE",
    "UTC_TIME",
    "UTC_TIMESTAMP",
    "VALUES",
    "VARBINARY",
    "VARCHAR",
    "VARCHARACTER",
    "VARYING",
    "VIRTUAL",
    "WHEN",
    "WHERE",
    "WHILE",
    "WINDOW",
    "WITH",
    "WRITE",
    "XOR",
    "YEAR_MONTH",
    "ZEROFILL",
];

/// The full Oracle reserved-word list (`docs/11`) – distinct from, and
/// much shorter than, Oracle's non-reserved Keywords list. Source: Oracle
/// Database SQL Language Reference, "Oracle SQL Reserved Words" (current
/// release 26ai as of 2026-09),
/// <https://docs.oracle.com/en/database/oracle/oracle-database/23/sqlrf/Oracle-SQL-Reserved-Words.html>
/// (fetched 2026-09-16). Excludes the `SYS_` naming-prefix convention and
/// the `V$RESERVED_WORDS` view mentioned in the surrounding prose –
/// neither is itself a reserved word.
pub const ORACLEDB_RESERVED_WORDS: &[&str] = &[
    "ACCESS",
    "ADD",
    "ALL",
    "ALTER",
    "AND",
    "ANY",
    "AS",
    "ASC",
    "AUDIT",
    "BETWEEN",
    "BY",
    "CHAR",
    "CHECK",
    "CLUSTER",
    "COLUMN",
    "COLUMN_VALUE",
    "COMMENT",
    "COMPRESS",
    "CONNECT",
    "CREATE",
    "CURRENT",
    "DATE",
    "DECIMAL",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "DROP",
    "ELSE",
    "EXCLUSIVE",
    "EXISTS",
    "FILE",
    "FLOAT",
    "FOR",
    "FROM",
    "GRANT",
    "GROUP",
    "HAVING",
    "IDENTIFIED",
    "IMMEDIATE",
    "IN",
    "INCREMENT",
    "INDEX",
    "INITIAL",
    "INSERT",
    "INTEGER",
    "INTERSECT",
    "INTO",
    "IS",
    "LEVEL",
    "LIKE",
    "LOCK",
    "LONG",
    "MAXEXTENTS",
    "MINUS",
    "MLSLABEL",
    "MODE",
    "MODIFY",
    "NESTED_TABLE_ID",
    "NOAUDIT",
    "NOCOMPRESS",
    "NOT",
    "NOWAIT",
    "NULL",
    "NUMBER",
    "OF",
    "OFFLINE",
    "ON",
    "ONLINE",
    "OPTION",
    "OR",
    "ORDER",
    "PCTFREE",
    "PRIOR",
    "PUBLIC",
    "RAW",
    "RENAME",
    "RESOURCE",
    "REVOKE",
    "ROW",
    "ROWID",
    "ROWNUM",
    "ROWS",
    "SELECT",
    "SESSION",
    "SET",
    "SHARE",
    "SIZE",
    "SMALLINT",
    "START",
    "SUCCESSFUL",
    "SYNONYM",
    "SYSDATE",
    "TABLE",
    "THEN",
    "TO",
    "TRIGGER",
    "UID",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USER",
    "VALIDATE",
    "VALUES",
    "VARCHAR",
    "VARCHAR2",
    "VIEW",
    "WHENEVER",
    "WHERE",
    "WITH",
];

/// The reserved-word list for `engine` – empty for an unknown engine.
/// Matching is case-insensitive (`docs/11`): a dialect's own case-folding
/// (postgres lowercases, oracledb uppercases, mysql preserves) doesn't
/// change whether a name is reserved.
pub fn reserved_words(engine: &str) -> &'static [&'static str] {
    match engine {
        "postgres" => POSTGRES_RESERVED_WORDS,
        "mysql" => MYSQL_RESERVED_WORDS,
        "oracledb" => ORACLEDB_RESERVED_WORDS,
        _ => &[],
    }
}

/// Identifier byte limits (`docs/11`). Postgres and MySQL are fixed;
/// Oracle's depends on the project's `identifierLength` setting.
pub const POSTGRES_IDENTIFIER_LIMIT: usize = 63;
pub const MYSQL_IDENTIFIER_LIMIT: usize = 64;
pub const ORACLEDB_IDENTIFIER_LIMIT_STANDARD: usize = 30;
pub const ORACLEDB_IDENTIFIER_LIMIT_EXTENDED: usize = 128;

/// Closed values of `Database { Dialect @oracledb { identifierLength = …; } }`
/// (`docs/11`, Project-level only).
pub const IDENTIFIER_LENGTH_VALUES: &[&str] = &["standard", "extended"];
