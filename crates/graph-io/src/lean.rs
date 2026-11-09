use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mathseea_model::{Edge, GraphDocument, GraphError, ImportGraph, ModuleNode};

fn path_error(path: &Path, message: impl std::fmt::Display) -> GraphError {
    GraphError::new(format!("{}: {message}", path.display()))
}

fn header_error(path: &Path, line: usize, message: impl std::fmt::Display) -> GraphError {
    path_error(path, format!("第 {line} 行: {message}"))
}

/// 只读取 Mathlib 树内的普通 Lean 文件, 不跟随符号链接.
fn module_files(root: &Path) -> Result<BTreeMap<String, PathBuf>, GraphError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| path_error(root, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(path_error(root, "扫描根必须为非符号链接目录"));
    }
    let root = fs::canonicalize(root).map_err(|error| path_error(root, error))?;
    let tree = if root.file_name().is_some_and(|name| name == "Mathlib") {
        root
    } else {
        root.join("Mathlib")
    };
    let mut pending = vec![tree.clone()];
    let mut files = BTreeMap::new();
    while let Some(directory) = pending.pop() {
        let metadata =
            fs::symlink_metadata(&directory).map_err(|error| path_error(&directory, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(path_error(&directory, "Mathlib 树不能包含符号链接目录"));
        }
        let mut entries = fs::read_dir(&directory)
            .map_err(|error| path_error(&directory, error))?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| path_error(&directory, error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        for path in entries {
            let metadata = fs::symlink_metadata(&path).map_err(|error| path_error(&path, error))?;
            if metadata.file_type().is_symlink() {
                return Err(path_error(&path, "Mathlib 树不能包含符号链接"));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "lean")
            {
                if !metadata.is_file() {
                    return Err(path_error(&path, "Lean 源必须为普通文件"));
                }
                let relative = path
                    .strip_prefix(&tree)
                    .map_err(|error| path_error(&path, error))?
                    .with_extension("");
                let mut segments = vec!["Mathlib".to_owned()];
                for component in relative.components() {
                    let name = component
                        .as_os_str()
                        .to_str()
                        .ok_or_else(|| path_error(&path, "模块路径不是有效 UTF-8"))?;
                    if name.is_empty() || name.chars().any(char::is_whitespace) {
                        return Err(path_error(&path, "模块路径含空名称或空白"));
                    }
                    segments.push(module_segment(name));
                }
                let name = segments.join(".");
                if files.insert(name.clone(), path.clone()).is_some() {
                    return Err(path_error(&path, format!("重复模块名称: {name}")));
                }
            }
        }
    }
    Ok(files)
}

fn identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

fn identifier_continue(character: char) -> bool {
    identifier_start(character)
        || character.is_numeric()
        || matches!(character, '\'' | '!' | '?' | '\u{2080}'..='\u{2089}')
}

fn module_segment(segment: &str) -> String {
    let mut characters = segment.chars();
    if characters.next().is_some_and(identifier_start)
        && characters.all(identifier_continue)
        && !keyword(segment)
    {
        segment.to_owned()
    } else {
        format!("«{segment}»")
    }
}

// Header parsing stops at command tokens, never searching later lines for imports.
// Escaped identifiers are names even when their contents spell a keyword.
fn keyword(word: &str) -> bool {
    matches!(
        word,
        "module"
            | "prelude"
            | "import"
            | "public"
            | "private"
            | "protected"
            | "meta"
            | "all"
            | "noncomputable"
            | "unsafe"
            | "partial"
            | "def"
            | "theorem"
            | "lemma"
            | "example"
            | "abbrev"
            | "opaque"
            | "axiom"
            | "axioms"
            | "constant"
            | "constants"
            | "inductive"
            | "coinductive"
            | "structure"
            | "class"
            | "instance"
            | "deriving"
            | "namespace"
            | "section"
            | "end"
            | "open"
            | "export"
            | "variable"
            | "variables"
            | "universe"
            | "universes"
            | "set_option"
            | "attribute"
            | "local"
            | "scoped"
            | "syntax"
            | "macro"
            | "macro_rules"
            | "elab"
            | "elab_rules"
            | "declare_syntax_cat"
            | "notation"
            | "infix"
            | "infixl"
            | "infixr"
            | "prefix"
            | "postfix"
            | "initialize"
            | "builtin_initialize"
            | "builtin_dsimproc"
            | "builtin_simproc"
            | "dsimproc"
            | "simproc"
            | "register_option"
            | "register_builtin_option"
            | "register_simp_attr"
            | "register_label_attr"
            | "declare_aesop_rule_sets"
            | "add_aesop_rules"
            | "assert_not_exists"
            | "assert_not_imported"
            | "run_cmd"
            | "run_elab"
            | "mutual"
            | "include"
            | "omit"
            | "in"
            | "where"
            | "with"
            | "by"
            | "do"
            | "if"
            | "then"
            | "else"
            | "match"
            | "let"
            | "fun"
            | "forall"
            | "return"
    )
}

#[derive(Clone, Debug)]
enum TokenKind {
    Name(String),
    Keyword(String),
    Other,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    line: usize,
    column: usize,
}

impl Token {
    fn is(&self, word: &str) -> bool {
        matches!(&self.kind, TokenKind::Keyword(value) if value == word)
    }
}

#[derive(Clone)]
struct HeaderLexer<'a> {
    remaining: &'a str,
    line: usize,
    column: usize,
    path: &'a Path,
}

impl<'a> HeaderLexer<'a> {
    fn new(text: &'a str, path: &'a Path) -> Self {
        Self {
            remaining: text.strip_prefix('\u{feff}').unwrap_or(text),
            line: 1,
            column: 1,
            path,
        }
    }

    fn consume(&mut self, bytes: usize) {
        let consumed = &self.remaining[..bytes];
        self.line += consumed.bytes().filter(|byte| *byte == b'\n').count();
        if let Some(last_newline) = consumed.rfind('\n') {
            self.column = consumed[last_newline + 1..].chars().count() + 1;
        } else {
            self.column += consumed.chars().count();
        }
        self.remaining = &self.remaining[bytes..];
    }

    fn skip_trivia(&mut self) -> Result<(), GraphError> {
        loop {
            let whitespace = self
                .remaining
                .find(|character: char| !character.is_whitespace())
                .unwrap_or(self.remaining.len());
            self.consume(whitespace);
            if self.remaining.starts_with("--") {
                let end = self.remaining.find('\n').unwrap_or(self.remaining.len());
                self.consume(end);
            } else if self.remaining.starts_with("/-") {
                // Documentation comments are commands, not header whitespace.
                if self.remaining.starts_with("/-!") || self.remaining.starts_with("/--") {
                    return Ok(());
                }
                let start_line = self.line;
                self.consume(2);
                let mut depth = 1_usize;
                while depth != 0 {
                    if self.remaining.is_empty() {
                        return Err(header_error(self.path, start_line, "未闭合块注释"));
                    }
                    if self.remaining.starts_with("/-") {
                        depth += 1;
                        self.consume(2);
                    } else if self.remaining.starts_with("-/") {
                        depth -= 1;
                        self.consume(2);
                    } else if let Some(character) = self.remaining.chars().next() {
                        self.consume(character.len_utf8());
                    }
                }
            } else {
                return Ok(());
            }
        }
    }

    fn segment(&mut self) -> Result<Option<String>, GraphError> {
        if self.remaining.starts_with('«') {
            let line = self.line;
            self.consume('«'.len_utf8());
            let Some(end) = self.remaining.find('»') else {
                return Err(header_error(self.path, line, "未闭合转义模块名称"));
            };
            let value = self.remaining[..end].to_owned();
            if value.is_empty() || value.chars().any(char::is_whitespace) {
                return Err(header_error(self.path, line, "模块名称为空或包含空白"));
            }
            self.consume(end + '»'.len_utf8());
            return Ok(Some(value));
        }
        if !self.remaining.chars().next().is_some_and(identifier_start) {
            return Ok(None);
        }
        let end = self
            .remaining
            .char_indices()
            .find(|(_, character)| !identifier_continue(*character))
            .map_or(self.remaining.len(), |(index, _)| index);
        let value = self.remaining[..end].to_owned();
        self.consume(end);
        Ok(Some(value))
    }

    fn next(&mut self) -> Result<Option<Token>, GraphError> {
        self.skip_trivia()?;
        if self.remaining.is_empty() {
            return Ok(None);
        }
        let line = self.line;
        let column = self.column;
        let escaped = self.remaining.starts_with('«');
        let Some(first) = self.segment()? else {
            // Do not lex strings, quotations or any part of a command body.
            self.consume(self.remaining.chars().next().map_or(0, char::len_utf8));
            return Ok(Some(Token {
                kind: TokenKind::Other,
                line,
                column,
            }));
        };
        if !escaped && keyword(&first) {
            return Ok(Some(Token {
                kind: TokenKind::Keyword(first),
                line,
                column,
            }));
        }
        let mut segments = vec![module_segment(&first)];
        while self.remaining.starts_with('.') {
            self.consume(1);
            let Some(segment) = self.segment()? else {
                return Err(header_error(self.path, line, "模块名称的点后缺少名称"));
            };
            segments.push(module_segment(&segment));
        }
        Ok(Some(Token {
            kind: TokenKind::Name(segments.join(".")),
            line,
            column,
        }))
    }

    fn peek(&self) -> Result<Option<Token>, GraphError> {
        self.clone().next()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct HeaderImport {
    name: String,
    line: usize,
}

fn header_imports(text: &str, path: &Path) -> Result<Vec<HeaderImport>, GraphError> {
    let mut lexer = HeaderLexer::new(text, path);
    for directive in ["module", "prelude"] {
        if lexer.peek()?.is_some_and(|token| token.is(directive)) {
            lexer.next()?;
        }
    }
    let mut imports = Vec::new();
    loop {
        let statement_column = lexer.peek()?.map_or(1, |token| token.column);
        let mut declaration = lexer.clone();
        let mut modifiers = BTreeSet::new();
        while let Some(token) = declaration.peek()? {
            let TokenKind::Keyword(word) = &token.kind else {
                break;
            };
            if !matches!(word.as_str(), "public" | "private" | "meta") {
                break;
            }
            if !modifiers.insert(word.clone()) {
                return Err(header_error(path, token.line, "重复 import 修饰符"));
            }
            declaration.next()?;
        }
        let Some(token) = declaration.next()? else {
            break;
        };
        if !token.is("import") {
            break;
        }
        if modifiers.contains("public") && modifiers.contains("private") {
            return Err(header_error(path, token.line, "import 可见性修饰符冲突"));
        }
        if declaration.peek()?.is_some_and(|token| token.is("all")) {
            declaration.next()?;
        }
        let before = imports.len();
        let mut last_module_line = token.line;
        while let Some(next) = declaration.peek()? {
            let TokenKind::Name(name) = next.kind else {
                break;
            };
            if imports.len() > before
                && next.line > last_module_line
                && next.column <= statement_column
            {
                return Err(header_error(
                    path,
                    next.line,
                    "无法区分模块续行与自定义正文命令; 请为模块单独写 import, 或缩进导入列表续行",
                ));
            }
            declaration.next()?;
            last_module_line = next.line;
            imports.push(HeaderImport {
                name,
                line: next.line,
            });
        }
        if imports.len() == before {
            return Err(header_error(path, token.line, "import 后缺少模块名称"));
        }
        lexer = declaration;
    }
    Ok(imports)
}

/// 在 Rust 内确定性扫描 Mathlib 源树, 只收集文件头的显式导入.
pub fn scan_mathlib(root: impl AsRef<Path>) -> Result<GraphDocument, GraphError> {
    let files = module_files(root.as_ref())?;
    let nodes = files.keys().map(ModuleNode::new).collect::<Vec<_>>();
    let ids = files
        .keys()
        .enumerate()
        .map(|(id, name)| (name.as_str(), id))
        .collect::<BTreeMap<_, _>>();
    let mut edges = BTreeSet::new();
    let mut external = BTreeMap::<String, BTreeSet<&str>>::new();
    for (name, path) in &files {
        let text = fs::read_to_string(path).map_err(|error| path_error(path, error))?;
        let dependent = ids[name.as_str()];
        for import in header_imports(&text, path)? {
            if import.name == "Mathlib" || import.name.starts_with("Mathlib.") {
                let Some(&dependency) = ids.get(import.name.as_str()) else {
                    return Err(header_error(
                        path,
                        import.line,
                        format!("缺失 Mathlib 内部模块: {}", import.name),
                    ));
                };
                if dependency == dependent {
                    return Err(header_error(path, import.line, "模块不能导入自身"));
                }
                edges.insert(Edge {
                    dependency,
                    dependent,
                });
            } else {
                external.entry(import.name).or_default().insert(name);
            }
        }
    }
    let graph = ImportGraph::new(nodes, edges.into_iter().collect())?;
    let warnings = external
        .into_iter()
        .map(|(name, sources)| {
            format!(
                "外部导入 {name} 未建立内部节点; 使用者: {}",
                sources.into_iter().collect::<Vec<_>>().join(", ")
            )
        })
        .collect();
    Ok(GraphDocument {
        graph,
        layout: None,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn header_imports_handle_comments_modifiers_and_multiline_modules() {
        let text = "\u{feff}/- outer /- nested -/ comment -/\nmodule\nprelude\npublic meta import Mathlib.A -- note\n Mathlib.B\nprivate import all Batteries\nimport Mathlib.«with».C\ndef body := \"import Mathlib.Fake\"\nimport Mathlib.TooLate\n";
        let imports = header_imports(text, Path::new("header.lean")).expect("header");
        assert_eq!(
            imports
                .iter()
                .map(|import| import.name.as_str())
                .collect::<Vec<_>>(),
            ["Mathlib.A", "Mathlib.B", "Batteries", "Mathlib.«with».C"]
        );
        assert_eq!(imports[0].line, 4);
        assert_eq!(imports[1].line, 5);
    }

    #[test]
    fn header_imports_do_not_search_bodies_strings_or_documentation() {
        for text in [
            "def text := \"import Mathlib.Fake\"\nimport Mathlib.Late",
            "#check \"import Mathlib.Fake\"",
            "namespace X\nimport Mathlib.Fake\nend X",
            "/-! import Mathlib.Fake -/\nimport Mathlib.Late",
            "/-- declaration docs -/\ndef x := \"import Mathlib.Fake\"",
        ] {
            assert!(
                header_imports(text, Path::new("body.lean"))
                    .expect("body")
                    .is_empty()
            );
        }
        let text = "import Mathlib.A\n/-! import Mathlib.Fake -/\nimport Mathlib.Late";
        let imports = header_imports(text, Path::new("body.lean")).expect("body");
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].name, "Mathlib.A");
    }

    #[test]
    fn header_imports_report_malformed_headers_with_path_and_line() {
        for text in [
            "\n/- unterminated",
            "\nimport",
            "\npublic public import Mathlib.A",
            "\npublic private import Mathlib.A",
            "\nimport Mathlib.",
            "\nimport Mathlib.«unterminated",
        ] {
            let error = header_imports(text, Path::new("bad.lean")).expect_err("invalid header");
            assert!(error.message.contains("bad.lean: 第 2 行"), "{error}");
        }
    }

    struct SourceTree(PathBuf);

    impl SourceTree {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "mathseea-graph-io-{stamp}-{}",
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).expect("fixture directory");
            fs::create_dir(root.join("Mathlib")).expect("Mathlib directory");
            Self(root)
        }

        fn source(&self, relative: &str, text: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("source parent");
            fs::write(path, text).expect("source");
        }
    }

    impl Drop for SourceTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scan_mathlib_is_sorted_internal_only_and_deduplicates_explicit_imports() {
        let tree = SourceTree::new();
        tree.source(
            "Mathlib/Z.lean",
            "public import Mathlib.A Mathlib.A\nimport Lean Batteries\ndef x := 1",
        );
        tree.source("Mathlib/A.lean", "prelude\nimport Lean");
        tree.source("Outside.lean", "import Mathlib.Missing");
        tree.source("Mathlib.lean", "import Mathlib.Missing");
        let document = scan_mathlib(&tree.0).expect("scan root");
        assert_eq!(
            document
                .graph
                .nodes
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["Mathlib.A", "Mathlib.Z"]
        );
        assert_eq!(
            document.graph.edges,
            [Edge {
                dependency: 0,
                dependent: 1
            }]
        );
        assert!(document.layout.is_none());
        assert_eq!(document.warnings.len(), 2);
        assert!(document.warnings[0].contains("Batteries"));
        assert!(document.warnings[1].contains("Mathlib.A, Mathlib.Z"));
        let repeated = scan_mathlib(tree.0.join("Mathlib/.")).expect("scan tree");
        assert_eq!(repeated.graph.edges, document.graph.edges);
        assert_eq!(repeated.warnings, document.warnings);
    }

    #[test]
    fn scan_mathlib_reports_missing_internal_imports_and_self_imports() {
        let tree = SourceTree::new();
        for import in ["Mathlib.Missing", "Mathlib", "Mathlib.A"] {
            tree.source("Mathlib/A.lean", &format!("\nimport {import}"));
            let error = scan_mathlib(&tree.0).expect_err("invalid internal import");
            assert!(error.message.contains("A.lean: 第 2 行"), "{error}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn scan_mathlib_rejects_symlink_directory_loops() {
        let tree = SourceTree::new();
        std::os::unix::fs::symlink(tree.0.join("Mathlib"), tree.0.join("Mathlib/Loop"))
            .expect("symlink fixture");
        let error = scan_mathlib(&tree.0).expect_err("symlink loop");
        assert!(error.message.contains("Loop"));
        assert!(error.message.contains("符号链接"));
    }
}
