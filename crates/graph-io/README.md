# mathseea-graph-io

读取和写入 MathSeea 文本图, 以及从本地 Lean 源码头部提取导入关系.

## 接口

```rust
pub fn parse_legacy(text: &str) -> Result<GraphDocument, GraphError>;
pub fn read_legacy(path: impl AsRef<Path>) -> Result<GraphDocument, GraphError>;
pub fn export_legacy(graph: &ImportGraph, layout: &Layout) -> Result<String, GraphError>;
pub fn write_legacy(
    path: impl AsRef<Path>,
    graph: &ImportGraph,
    layout: &Layout,
) -> Result<(), GraphError>;
pub fn scan_mathlib(root: impl AsRef<Path>) -> Result<GraphDocument, GraphError>;
```

读取文本图时保留节点顺序、名称、RGB、坐标和半径. 布局坐标使用 `f32`.

## 文本图格式

```text
N
name #RRGGBB x y radius
...
dependency_id dependent_count dependent_id...
```

前 N 行是节点, 后 N 行是邻接表. 邻接行表示依赖项指向其使用者. 若 B 导入 A, 则 A 出现在 B 的依赖列表中. 解析器会检查行数、字段、索引、重名、重复边、坐标和半径.

`export_legacy` 只序列化图与布局, 不写文件. `write_legacy` 是库级覆盖写入接口; 桌面应用导出时应使用 `export_legacy` 配合 `create_new` 避免覆盖已有文件.

## 扫描 Mathlib

```rust
let document = mathseea_graph_io::scan_mathlib("/path/to/mathlib4")?;
```

`scan_mathlib` 接受 mathlib4 根目录或 `Mathlib` 子目录, 只读取普通 `.lean` 文件, 不跟随符号链接. 它解析文件头的 `module`, `prelude`, `import` 及 `public`/`meta` 等修饰符, 处理普通块注释、行注释、转义模块名和多模块导入.

扫描结果是 `GraphDocument { layout: None }`; 布局由 `mathseea-graph-engine` 生成. 缺失内部模块会返回带路径和行号的错误, Lean、Batteries 等外部导入只记录为说明.

扫描器不会执行 Lean, 不检查证明, 不读取 `.olean`, 不补齐隐式依赖. 文档注释或正文命令开始后停止扫描. 多模块导入的换行续行需要缩进; 无法区分的语法会返回诊断, 不会猜测导入关系.

## 构建

```sh
cargo check -p mathseea-graph-io --locked
```
