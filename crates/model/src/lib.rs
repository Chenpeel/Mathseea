//! 公共图模型. 边统一从依赖项指向使用该依赖的模块.

use std::collections::{BTreeSet, HashSet};
use std::fmt;

pub type NodeId = usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphError {
    pub message: String,
}

impl GraphError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GraphError {}

impl From<std::io::Error> for GraphError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleNode {
    pub name: String,
    pub topic: String,
    pub color: Rgb,
}

impl ModuleNode {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let topic = name
            .strip_prefix("Mathlib.")
            .unwrap_or(&name)
            .split('.')
            .next()
            .unwrap_or("Other")
            .to_owned();
        let color = topic_color(&topic);
        Self { name, topic, color }
    }
}

/// dependency -> dependent. 若 B 导入 A, 则存储 A -> B.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    pub dependency: NodeId,
    pub dependent: NodeId,
}

#[derive(Clone, Debug)]
pub struct ImportGraph {
    pub nodes: Vec<ModuleNode>,
    pub edges: Vec<Edge>,
    /// 按节点索引存储其直接导入的模块.
    pub dependencies: Vec<Vec<NodeId>>,
    /// 按节点索引存储直接导入该模块的使用者.
    pub dependents: Vec<Vec<NodeId>>,
}

impl ImportGraph {
    pub fn new(nodes: Vec<ModuleNode>, mut edges: Vec<Edge>) -> Result<Self, GraphError> {
        let mut names = HashSet::with_capacity(nodes.len());
        for node in &nodes {
            if node.name.is_empty() || node.name.chars().any(char::is_whitespace) {
                return Err(GraphError::new(format!("无效模块名称: {:?}", node.name)));
            }
            if !names.insert(node.name.as_str()) {
                return Err(GraphError::new(format!("重复模块名称: {}", node.name)));
            }
            if node.topic.is_empty() {
                return Err(GraphError::new(format!("模块主题为空: {}", node.name)));
            }
        }
        edges.sort_unstable();
        let mut previous = None;
        let mut dependencies = vec![Vec::new(); nodes.len()];
        let mut dependents = vec![Vec::new(); nodes.len()];
        for &edge in &edges {
            if edge.dependency >= nodes.len() || edge.dependent >= nodes.len() {
                return Err(GraphError::new(format!("连边索引越界: {edge:?}")));
            }
            if previous == Some(edge) {
                return Err(GraphError::new(format!("重复连边: {edge:?}")));
            }
            if edge.dependency == edge.dependent {
                return Err(GraphError::new(format!(
                    "模块存在自循环: {}",
                    nodes[edge.dependency].name
                )));
            }
            dependencies[edge.dependent].push(edge.dependency);
            dependents[edge.dependency].push(edge.dependent);
            previous = Some(edge);
        }
        Ok(Self {
            nodes,
            edges,
            dependencies,
            dependents,
        })
    }

    pub fn validate(&self) -> Result<(), GraphError> {
        let rebuilt = Self::new(self.nodes.clone(), self.edges.clone())?;
        if rebuilt.edges != self.edges
            || rebuilt.dependencies != self.dependencies
            || rebuilt.dependents != self.dependents
        {
            return Err(GraphError::new("图的邻接索引与连边不一致"));
        }
        Ok(())
    }

    pub fn topics(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .map(|node| node.topic.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodePlacement {
    pub position: Point,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopicLabel {
    pub topic: String,
    pub position: Point,
    pub color: Rgb,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub nodes: Vec<NodePlacement>,
    pub topics: Vec<TopicLabel>,
    pub bounds: Bounds,
}

impl Layout {
    pub fn from_nodes(graph: &ImportGraph, nodes: Vec<NodePlacement>) -> Result<Self, GraphError> {
        if nodes.len() != graph.nodes.len() {
            return Err(GraphError::new("布局节点数量与图不一致"));
        }
        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for placement in &nodes {
            if !placement.position.x.is_finite()
                || !placement.position.y.is_finite()
                || !placement.radius.is_finite()
                || placement.radius <= 0.0
            {
                return Err(GraphError::new("布局包含非有限坐标或非正半径"));
            }
            min.x = min.x.min(placement.position.x - placement.radius);
            min.y = min.y.min(placement.position.y - placement.radius);
            max.x = max.x.max(placement.position.x + placement.radius);
            max.y = max.y.max(placement.position.y + placement.radius);
        }
        if nodes.is_empty() {
            min = Point::new(0.0, 0.0);
            max = Point::new(1.0, 1.0);
        }
        if !min.x.is_finite() || !min.y.is_finite() || !max.x.is_finite() || !max.y.is_finite() {
            return Err(GraphError::new("布局边界超出有限坐标范围"));
        }
        let mut topics = Vec::new();
        for topic in graph.topics() {
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            let mut color = topic_color(topic);
            for (id, node) in graph.nodes.iter().enumerate() {
                if node.topic == topic {
                    xs.push(nodes[id].position.x);
                    ys.push(nodes[id].position.y);
                    color = node.color;
                }
            }
            xs.sort_by(f32::total_cmp);
            ys.sort_by(f32::total_cmp);
            topics.push(TopicLabel {
                topic: topic.to_owned(),
                position: Point::new(xs[xs.len() / 2], ys[ys.len() / 2]),
                color,
            });
        }
        Ok(Self {
            nodes,
            topics,
            bounds: Bounds { min, max },
        })
    }

    pub fn validate(&self, graph: &ImportGraph) -> Result<(), GraphError> {
        let rebuilt = Self::from_nodes(graph, self.nodes.clone())?;
        if self.bounds != rebuilt.bounds {
            return Err(GraphError::new("布局边界与节点坐标不一致"));
        }
        for label in &self.topics {
            if !label.position.x.is_finite()
                || !label.position.y.is_finite()
                || !graph.nodes.iter().any(|node| node.topic == label.topic)
            {
                return Err(GraphError::new("主题标签包含非法坐标或未知主题"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct GraphDocument {
    pub graph: ImportGraph,
    pub layout: Option<Layout>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    #[default]
    None,
    Node(NodeId),
    Topic(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HighlightMode {
    #[default]
    Neighbors,
    Dependencies,
    Dependents,
    TopicMembers,
    TopicDependencies,
    TopicDependents,
}

impl HighlightMode {
    pub fn is_topic(self) -> bool {
        matches!(
            self,
            Self::TopicMembers | Self::TopicDependencies | Self::TopicDependents
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Neighbors => "直接邻居",
            Self::Dependencies => "传递依赖",
            Self::Dependents => "传递使用者",
            Self::TopicMembers => "主题内节点",
            Self::TopicDependencies => "主题直接依赖",
            Self::TopicDependents => "直接引用主题",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Highlight {
    pub nodes: Vec<bool>,
    pub edges: Vec<bool>,
    pub active: bool,
}

impl Highlight {
    pub fn all(graph: &ImportGraph) -> Self {
        Self {
            nodes: vec![true; graph.nodes.len()],
            edges: vec![true; graph.edges.len()],
            active: false,
        }
    }

    pub fn none(graph: &ImportGraph) -> Self {
        Self {
            nodes: vec![false; graph.nodes.len()],
            edges: vec![false; graph.edges.len()],
            active: true,
        }
    }
}

/// 独立定义主题调色板, 不读取原项目字体, 着色器或程序资源.
pub fn topic_color(topic: &str) -> Rgb {
    match topic {
        "Algebra" => Rgb::new(240, 221, 60),
        "Analysis" => Rgb::new(35, 215, 220),
        "Topology" => Rgb::new(224, 58, 213),
        "LinearAlgebra" => Rgb::new(80, 220, 97),
        "CategoryTheory" => Rgb::new(135, 169, 244),
        "NumberTheory" => Rgb::new(206, 76, 99),
        "RingTheory" => Rgb::new(239, 143, 62),
        "GroupTheory" => Rgb::new(236, 73, 125),
        "MeasureTheory" => Rgb::new(164, 99, 235),
        "Probability" => Rgb::new(87, 128, 245),
        "Logic" => Rgb::new(70, 162, 241),
        "SetTheory" => Rgb::new(234, 132, 142),
        "Geometry" => Rgb::new(223, 147, 232),
        "FieldTheory" => Rgb::new(226, 224, 136),
        "Tactic" => Rgb::new(110, 119, 170),
        "Data" => Rgb::new(133, 145, 157),
        _ => {
            let hash = topic.bytes().fold(2_166_136_261_u32, |hash, byte| {
                (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
            });
            Rgb::new(
                90 + (hash & 127) as u8,
                90 + ((hash >> 8) & 127) as u8,
                90 + ((hash >> 16) & 127) as u8,
            )
        }
    }
}
