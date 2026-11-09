use mathseea_model::{Bounds, GraphError, Highlight, ImportGraph, Layout, Selection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BufferIdentity {
    address: usize,
    len: usize,
}

impl BufferIdentity {
    fn of<T>(items: &[T]) -> Self {
        Self {
            address: items.as_ptr() as usize,
            len: items.len(),
        }
    }
}

/// Loaded documents are immutable until reset_camera invalidates this identity cache.
#[derive(Debug)]
pub(crate) struct DocumentIdentity {
    buffers: [BufferIdentity; 6],
    dependencies: Vec<BufferIdentity>,
    dependents: Vec<BufferIdentity>,
    bounds: Bounds,
}

impl DocumentIdentity {
    fn buffers(graph: &ImportGraph, layout: &Layout) -> [BufferIdentity; 6] {
        [
            BufferIdentity::of(&graph.nodes),
            BufferIdentity::of(&graph.edges),
            BufferIdentity::of(&graph.dependencies),
            BufferIdentity::of(&graph.dependents),
            BufferIdentity::of(&layout.nodes),
            BufferIdentity::of(&layout.topics),
        ]
    }

    pub(crate) fn new(graph: &ImportGraph, layout: &Layout) -> Self {
        Self {
            buffers: Self::buffers(graph, layout),
            dependencies: graph
                .dependencies
                .iter()
                .map(|items| BufferIdentity::of(items))
                .collect(),
            dependents: graph
                .dependents
                .iter()
                .map(|items| BufferIdentity::of(items))
                .collect(),
            bounds: layout.bounds,
        }
    }

    pub(crate) fn matches(&self, graph: &ImportGraph, layout: &Layout) -> bool {
        self.buffers == Self::buffers(graph, layout)
            && self.bounds == layout.bounds
            && self.dependencies.iter().copied().eq(graph
                .dependencies
                .iter()
                .map(|items| BufferIdentity::of(items)))
            && self.dependents.iter().copied().eq(graph
                .dependents
                .iter()
                .map(|items| BufferIdentity::of(items)))
    }
}

pub(crate) fn validate_document(graph: &ImportGraph, layout: &Layout) -> Result<(), GraphError> {
    graph.validate()?;
    layout.validate(graph)
}

pub(crate) fn validate_frame(
    graph: &ImportGraph,
    layout: &Layout,
    highlight: &Highlight,
    selection: &Selection,
) -> Result<(), GraphError> {
    if layout.nodes.len() != graph.nodes.len()
        || graph.dependencies.len() != graph.nodes.len()
        || graph.dependents.len() != graph.nodes.len()
    {
        return Err(GraphError::new("图与布局的节点数组长度不一致"));
    }
    if highlight.nodes.len() != graph.nodes.len() || highlight.edges.len() != graph.edges.len() {
        return Err(GraphError::new("高亮数组长度与图不一致"));
    }
    match selection {
        Selection::Node(id) if *id >= graph.nodes.len() => {
            Err(GraphError::new(format!("所选节点索引越界: {id}")))
        }
        Selection::Topic(topic) if !graph.nodes.iter().any(|node| node.topic == *topic) => {
            Err(GraphError::new(format!("所选主题不存在: {topic}")))
        }
        _ => Ok(()),
    }
}
