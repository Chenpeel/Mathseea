use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use mathseea_model::{
    Edge, GraphDocument, GraphError, ImportGraph, Layout, ModuleNode, NodePlacement, Point, Rgb,
};

fn line_error(line: usize, message: impl std::fmt::Display) -> GraphError {
    GraphError::new(format!("第 {line} 行: {message}"))
}

fn unsigned(field: &str, line: usize, label: &str) -> Result<usize, GraphError> {
    if field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(line_error(line, format!("{label}不是非负整数: {field:?}")));
    }
    field
        .parse()
        .map_err(|_| line_error(line, format!("{label}超出索引范围: {field:?}")))
}

fn coordinate(field: &str, line: usize, label: &str) -> Result<f32, GraphError> {
    let value: f32 = field
        .parse()
        .map_err(|_| line_error(line, format!("无效{label}: {field:?}")))?;
    if !value.is_finite() {
        return Err(line_error(line, format!("{label}必须有限: {field:?}")));
    }
    Ok(value)
}

fn color(field: &str, line: usize) -> Result<Rgb, GraphError> {
    let Some(hex) = field.strip_prefix('#') else {
        return Err(line_error(line, "颜色必须为 #RRGGBB"));
    };
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(line_error(line, "颜色必须为 #RRGGBB"));
    }
    let value = u32::from_str_radix(hex, 16).map_err(|_| line_error(line, "颜色必须为 #RRGGBB"))?;
    Ok(Rgb::new(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

/// 读取 N 行节点与 N 行邻接表. 每个邻接记录为 dependency、数量、dependents.
pub fn parse_legacy(text: &str) -> Result<GraphDocument, GraphError> {
    let mut lines = text.lines().enumerate();
    let Some((_, header)) = lines.next() else {
        return Err(line_error(1, "缺少节点数量"));
    };
    let header: Vec<_> = header.split_whitespace().collect();
    if header.len() != 1 {
        return Err(line_error(1, "节点数量行必须恰有一个字段"));
    }
    let count = unsigned(header[0], 1, "节点数量")?;
    let expected = count
        .checked_mul(2)
        .ok_or_else(|| line_error(1, "节点数量过大"))?;
    // 先核对行数, 避免被不可信的数量字段触发巨量分配.
    if lines.clone().count() != expected {
        return Err(line_error(
            1,
            format!("节点数量 {count} 要求后续恰有 {expected} 行节点及邻接记录"),
        ));
    }
    let mut nodes = Vec::with_capacity(count);
    let mut placements = Vec::with_capacity(count);
    let mut names = HashSet::with_capacity(count);
    for _ in 0..count {
        let (offset, record) = lines.next().ok_or_else(|| line_error(1, "缺少节点记录"))?;
        let line = offset + 1;
        let fields: Vec<_> = record.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(line_error(line, "节点记录必须为 名称 #RRGGBB x y radius"));
        }
        if !names.insert(fields[0]) {
            return Err(line_error(line, format!("重复模块名称: {}", fields[0])));
        }
        let mut node = ModuleNode::new(fields[0]);
        node.color = color(fields[1], line)?;
        let position = Point::new(
            coordinate(fields[2], line, "x 坐标")?,
            coordinate(fields[3], line, "y 坐标")?,
        );
        let radius = coordinate(fields[4], line, "半径")?;
        if radius <= 0.0 {
            return Err(line_error(line, "半径必须为正"));
        }
        nodes.push(node);
        placements.push(NodePlacement { position, radius });
    }
    let mut seen = vec![false; count];
    let mut edges = Vec::new();
    for (offset, record) in lines {
        let line = offset + 1;
        let fields: Vec<_> = record.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(line_error(line, "邻接记录缺少索引或数量"));
        }
        let dependency = unsigned(fields[0], line, "节点索引")?;
        if dependency >= count {
            return Err(line_error(line, format!("节点索引越界: {dependency}")));
        }
        if std::mem::replace(&mut seen[dependency], true) {
            return Err(line_error(line, format!("重复邻接记录: {dependency}")));
        }
        let degree = unsigned(fields[1], line, "邻接数量")?;
        if degree != fields.len() - 2 {
            return Err(line_error(line, "邻接数量与索引字段数不一致"));
        }
        let mut dependents = HashSet::with_capacity(degree);
        for field in &fields[2..] {
            let dependent = unsigned(field, line, "使用者索引")?;
            if dependent >= count {
                return Err(line_error(line, format!("使用者索引越界: {dependent}")));
            }
            if dependency == dependent {
                return Err(line_error(line, format!("模块存在自循环: {dependent}")));
            }
            if !dependents.insert(dependent) {
                return Err(line_error(line, format!("重复使用者索引: {dependent}")));
            }
            edges.push(Edge {
                dependency,
                dependent,
            });
        }
    }
    let graph = ImportGraph::new(nodes, edges)?;
    let layout = Layout::from_nodes(&graph, placements)?;
    Ok(GraphDocument {
        graph,
        layout: Some(layout),
        warnings: Vec::new(),
    })
}

pub fn read_legacy(path: impl AsRef<Path>) -> Result<GraphDocument, GraphError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .map_err(|error| GraphError::new(format!("{}: {error}", path.display())))?;
    parse_legacy(&text).map_err(|error| GraphError::new(format!("{}: {error}", path.display())))
}

/// 保留节点顺序与 f32 布局值, 按依赖项索引和使用者索引确定性输出.
pub fn export_legacy(graph: &ImportGraph, layout: &Layout) -> Result<String, GraphError> {
    graph.validate()?;
    layout.validate(graph)?;
    let mut text = String::new();
    // 对 String 的格式写入不会发生 IO 错误; 仍显式传播 fmt 错误而非 panic.
    let format_error = |_| GraphError::new("无法格式化原文本图");
    writeln!(text, "{}", graph.nodes.len()).map_err(format_error)?;
    for (node, placement) in graph.nodes.iter().zip(&layout.nodes) {
        writeln!(
            text,
            "{} #{:02x}{:02x}{:02x} {} {} {}",
            node.name,
            node.color.r,
            node.color.g,
            node.color.b,
            placement.position.x,
            placement.position.y,
            placement.radius,
        )
        .map_err(format_error)?;
    }
    for (dependency, dependents) in graph.dependents.iter().enumerate() {
        write!(text, "{dependency} {}", dependents.len()).map_err(format_error)?;
        for dependent in dependents {
            write!(text, " {dependent}").map_err(format_error)?;
        }
        text.push('\n');
    }
    Ok(text)
}

pub fn write_legacy(
    path: impl AsRef<Path>,
    graph: &ImportGraph,
    layout: &Layout,
) -> Result<(), GraphError> {
    let text = export_legacy(graph, layout)?;
    let path = path.as_ref();
    fs::write(path, text).map_err(|error| GraphError::new(format!("{}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAPH: &str = "3\nMathlib.ApplyWith #0123af -0 2.5 1\nMathlib.Tactic #ff0000 7 -8 0.25\nMathlib.Other #00ff00 11 12 2\n0 2 2 1\n1 0\n2 0\n";

    #[test]
    fn parse_legacy_successors_point_from_dependency_to_dependent() {
        let document = parse_legacy(GRAPH).expect("valid graph");
        assert_eq!(document.graph.nodes[0].name, "Mathlib.ApplyWith");
        assert_eq!(document.graph.dependents[0], [1, 2]);
        assert_eq!(document.graph.dependencies[1], [0]);
        let layout = document.layout.as_ref().expect("legacy layout");
        assert_eq!(layout.nodes[0].position.x.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(layout.nodes[1].position, Point::new(7.0, -8.0));
        let exported = export_legacy(&document.graph, layout).expect("export");
        assert!(exported.ends_with("0 2 1 2\n1 0\n2 0\n"));
        let reparsed = parse_legacy(&exported).expect("round trip");
        assert_eq!(reparsed.graph.edges, document.graph.edges);
        assert_eq!(reparsed.layout, document.layout);
    }

    #[test]
    fn parse_legacy_rejects_invalid_fields_counts_and_duplicates() {
        for invalid in [
            GRAPH.replacen("3\n", "+3\n", 1),
            GRAPH.replace("#0123af", "#xyzxyz"),
            GRAPH.replace("-0 2.5 1", "NaN 2.5 1"),
            GRAPH.replace("-0 2.5 1", "0 inf 1"),
            GRAPH.replace("-0 2.5 1", "0 2.5 0"),
            GRAPH.replace("Mathlib.Other", "Mathlib.Tactic"),
            GRAPH.replace("0 2 2 1", "0 1 2 1"),
            GRAPH.replace("0 2 2 1", "0 2 1 1"),
            GRAPH.replace("0 2 2 1", "0 2 2 3"),
            GRAPH.replace("0 2 2 1", "0 2 2 0"),
            GRAPH.replace("1 0\n2 0", "1 0\n1 0"),
            format!("{GRAPH}\n"),
            format!("{GRAPH}extra\n"),
        ] {
            assert!(parse_legacy(&invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn export_legacy_validates_public_graph_and_layout() {
        let mut document = parse_legacy(GRAPH).expect("valid graph");
        let mut layout = document.layout.take().expect("layout");
        layout.nodes[0].radius = f32::INFINITY;
        assert!(export_legacy(&document.graph, &layout).is_err());
        document.graph.dependencies[1].clear();
        assert!(export_legacy(&document.graph, &Layout::default()).is_err());
    }
}
