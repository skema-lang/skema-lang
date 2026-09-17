//! Print the syntax tree as indented text — readable where `{:#?}` is not.
//! See `examples/dump_ast.rs`.

use crate::ast::{self, BlockKind, FieldTypeKind, ItemKind, ValueKind};
use std::fmt::Write as _;

/// The whole tree as readable text.
pub fn dump(file: &ast::SourceFile) -> String {
    let mut out = String::new();
    for item in file.items() {
        item.write(&mut out, 0);
    }
    out
}

/// Writes one line, indented by `depth` × two spaces.
///
/// `let _ =` because `write!` into a `String` can't fail — but we're under
/// `unwrap_used = "warn"` and don't want the noise.
fn line(out: &mut String, depth: usize, text: &str) {
    let _ = writeln!(out, "{:indent$}{text}", "", indent = depth * 2);
}

/// A shared trait so every node knows its own output.
///
/// A trait instead of one big `match` function: this way each node's output
/// lives with the node itself, and a new node can't forget to implement it —
/// the compiler demands it.
trait Dump {
    fn write(&self, out: &mut String, depth: usize);
}

impl Dump for ast::Item {
    fn write(&self, out: &mut String, depth: usize) {
        match self.kind() {
            ItemKind::Spec(s) => line(
                out,
                depth,
                &format!("spec {}", opt(s.version().map(|v| v.to_string()))),
            ),
            ItemKind::Import(i) => {
                let alias = match i.alias() {
                    Some(a) => format!(" as {}", a.text()),
                    None => String::new(),
                };
                line(
                    out,
                    depth,
                    &format!("import {:?}{alias}", i.path().as_deref().unwrap_or("?")),
                );
            }
            ItemKind::Use(u) => line(out, depth, &format!("use {}", u.target().to_dotted())),
            ItemKind::Setting(a) => line(out, depth, &format!("setting {}", attr(&a))),
            ItemKind::Field(f) => f.write(out, depth),
            ItemKind::Block(b) => b.write(out, depth),
            ItemKind::Ref(r) => line(
                out,
                depth,
                &format!("ref {} {}", r.kind().as_str(), r.path().to_dotted()),
            ),
            ItemKind::IndexEntry(e) => {
                let columns: Vec<_> = e.columns().map(|c| c.text().to_owned()).collect();
                let name = match e.name() {
                    Some(n) => format!("{} ", n.text()),
                    None => String::new(),
                };
                let flags: Vec<_> = e.flags().collect();
                line(
                    out,
                    depth,
                    &format!("index {name}({}){}", columns.join(", "), flags_text(&flags)),
                );
            }
            ItemKind::Error => line(out, depth, "<broken section>"),
        }
    }
}

impl Dump for ast::Block {
    fn write(&self, out: &mut String, depth: usize) {
        let mut head = format!("Block {}", self.kind().as_str());
        if self.kind() == BlockKind::Other {
            let _ = write!(head, "({})", self.keyword().text());
        }
        if let Some(s) = self.selector() {
            let _ = write!(head, " @{}", s.text());
        }
        if let Some(n) = self.name() {
            let _ = write!(head, " {}", n.to_dotted());
        }
        let params: Vec<_> = self.params().collect();
        if !params.is_empty() {
            let p: Vec<_> = params.iter().map(param).collect();
            let _ = write!(head, "({})", p.join(", "));
        }
        line(out, depth, &head);
        for item in self.items() {
            item.write(out, depth + 1);
        }
    }
}

impl Dump for ast::Field {
    fn write(&self, out: &mut String, depth: usize) {
        let mut head = format!("Field {}", self.name().text());
        if let Some(t) = self.ty() {
            let _ = write!(head, ": {}", field_type(&t));
        }
        let flags: Vec<_> = self.flags().collect();
        let _ = write!(head, "{}", flags_text(&flags));
        line(out, depth, &head);
        for item in self.items() {
            item.write(out, depth + 1);
        }
    }
}

// -- Small pieces as one-liners ----------------------------------------------

fn field_type(t: &ast::FieldType) -> String {
    match t.kind() {
        Some(FieldTypeKind::Named { path, args }) if args.is_empty() => path.to_dotted(),
        Some(FieldTypeKind::Named { path, args }) => {
            let a: Vec<_> = args.iter().map(value).collect();
            format!("{}({})", path.to_dotted(), a.join(", "))
        }
        Some(FieldTypeKind::Relation { op, target }) => {
            format!("{}{}", op.as_str(), target.to_dotted())
        }
        None => String::new(),
    }
}

fn param(p: &ast::Param) -> String {
    let name = p.name().map(|n| n.text().to_owned()).unwrap_or_default();
    match p.ty() {
        Some(ty) => format!("{}: {}", name, ty.to_dotted()),
        None => name,
    }
}

fn flags_text(flags: &[ast::Attr]) -> String {
    if flags.is_empty() {
        return String::new();
    }
    let f: Vec<_> = flags.iter().map(attr).collect();
    format!(" [{}]", f.join(", "))
}

fn attr(a: &ast::Attr) -> String {
    let key = a.key();
    let key_text = if key.quoted() {
        format!("{:?}", key.text())
    } else {
        key.text()
    };
    match a.value() {
        Some(v) => format!("{key_text} = {}", value(&v)),
        None => key_text,
    }
}

fn value(v: &ast::Value) -> String {
    match v.kind() {
        Some(ValueKind::Path(p)) => p.to_dotted(),
        Some(ValueKind::Str(s)) => format!("{s:?}"),
        Some(ValueKind::Int(n)) => n.to_string(),
        Some(ValueKind::Float(f)) => f.to_string(),
        Some(ValueKind::Raw(r)) => format!("`{r}`"),
        Some(ValueKind::List(items)) => {
            let i: Vec<_> = items.iter().map(value).collect();
            format!("[{}]", i.join(", "))
        }
        Some(ValueKind::Type { path, args }) => {
            let a: Vec<_> = args.iter().map(value).collect();
            format!("{}({})", path.to_dotted(), a.join(", "))
        }
        None => String::new(),
    }
}

fn opt(v: Option<String>) -> String {
    v.unwrap_or_else(|| "?".to_owned())
}
