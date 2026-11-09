use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use mathseea_graph_engine::{compute_layout, topological_order};
use mathseea_graph_io::{export_legacy, parse_legacy, read_legacy, scan_mathlib};
use mathseea_model::{GraphError, ImportGraph, Layout, Rgb};

use crate::cli::Source;

/// 完成载入后保持图和布局不可变, 避免使视图校验缓存失效.
pub struct LoadedGraph {
    pub graph: ImportGraph,
    pub layout: Layout,
    pub source: Source,
    pub warnings: Vec<String>,
    pub search_names: Vec<String>,
    pub topics: Vec<(String, usize, Rgb)>,
}

pub fn load(source: Source) -> Result<LoadedGraph, GraphError> {
    let document = match &source {
        Source::Bundled => parse_legacy(include_str!("../../../data/import_graph.txt"))?,
        Source::Graph(path) => read_legacy(path)?,
        Source::Mathlib(path) => scan_mathlib(path)?,
    };
    let graph = document.graph;
    let layout = if let Some(layout) = document.layout {
        topological_order(&graph)?;
        layout.validate(&graph)?;
        for edge in &graph.edges {
            if layout.nodes[edge.dependency].position.x >= layout.nodes[edge.dependent].position.x {
                return Err(GraphError::new(format!(
                    "图文件布局未满足依赖从左向右: {} -> {}",
                    graph.nodes[edge.dependency].name, graph.nodes[edge.dependent].name
                )));
            }
        }
        layout
    } else {
        compute_layout(&graph)?
    };
    let mut topics = BTreeMap::new();
    let search_names = graph
        .nodes
        .iter()
        .map(|node| {
            let entry = topics
                .entry(node.topic.clone())
                .or_insert((0_usize, node.color));
            entry.0 += 1;
            node.name.to_lowercase()
        })
        .collect();
    Ok(LoadedGraph {
        graph,
        layout,
        source,
        warnings: document.warnings,
        search_names,
        topics: topics
            .into_iter()
            .map(|(name, (count, color))| (name, count, color))
            .collect(),
    })
}

/// 先完成序列化再创建新文件, 拒绝覆盖现有文件和符号链接.
pub fn export_new(path: &Path, document: &LoadedGraph) -> Result<(), GraphError> {
    let text = export_legacy(&document.graph, &document.layout)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| GraphError::new(format!("无法新建 {}: {error}", path.display())))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            GraphError::new(format!(
                "写入 {} 失败, 该路径可能保留部分内容: {error}",
                path.display()
            ))
        })
}
