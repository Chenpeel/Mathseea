use mathseea_model::{GraphError, Highlight, HighlightMode, ImportGraph, NodeId, Selection};

/// 按节点或主题的关系语义生成与原图数组一一对应的高亮掩码.
pub fn highlight(
    graph: &ImportGraph,
    selection: &Selection,
    mode: HighlightMode,
) -> Result<Highlight, GraphError> {
    graph.validate()?;
    match selection {
        Selection::None => Ok(Highlight::all(graph)),
        Selection::Node(id) => {
            if *id >= graph.nodes.len() {
                return Err(GraphError::new(format!("选择的节点 ID 越界: {id}")));
            }
            if mode.is_topic() {
                return Err(GraphError::new("节点选择不能使用主题高亮模式"));
            }
            Ok(node_highlight(graph, *id, mode))
        }
        Selection::Topic(topic) => {
            if !graph.nodes.iter().any(|node| node.topic == *topic) {
                return Err(GraphError::new(format!("选择的主题不存在: {topic}")));
            }
            if !mode.is_topic() {
                return Err(GraphError::new("主题选择不能使用节点高亮模式"));
            }
            Ok(topic_highlight(graph, topic, mode))
        }
    }
}

fn node_highlight(graph: &ImportGraph, seed: NodeId, mode: HighlightMode) -> Highlight {
    let mut result = Highlight::none(graph);
    result.nodes[seed] = true;
    match mode {
        HighlightMode::Neighbors => {
            for &id in graph.dependencies[seed]
                .iter()
                .chain(&graph.dependents[seed])
            {
                result.nodes[id] = true;
            }
            for (index, edge) in graph.edges.iter().enumerate() {
                result.edges[index] = edge.dependency == seed || edge.dependent == seed;
            }
        }
        HighlightMode::Dependencies | HighlightMode::Dependents => {
            let adjacency = if mode == HighlightMode::Dependencies {
                &graph.dependencies
            } else {
                &graph.dependents
            };
            let mut pending = vec![seed];
            while let Some(id) = pending.pop() {
                for &neighbor in &adjacency[id] {
                    if !result.nodes[neighbor] {
                        result.nodes[neighbor] = true;
                        pending.push(neighbor);
                    }
                }
            }
            for (index, edge) in graph.edges.iter().enumerate() {
                result.edges[index] = result.nodes[edge.dependency] && result.nodes[edge.dependent];
            }
        }
        _ => unreachable!("公开入口已拒绝节点选择的主题模式"),
    }
    result
}

fn topic_highlight(graph: &ImportGraph, topic: &str, mode: HighlightMode) -> Highlight {
    let mut result = Highlight::none(graph);
    for (id, node) in graph.nodes.iter().enumerate() {
        result.nodes[id] = node.topic == topic;
    }
    for (index, edge) in graph.edges.iter().enumerate() {
        let dependency_in_topic = graph.nodes[edge.dependency].topic == topic;
        let dependent_in_topic = graph.nodes[edge.dependent].topic == topic;
        let matches = match mode {
            HighlightMode::TopicMembers => dependency_in_topic && dependent_in_topic,
            HighlightMode::TopicDependencies => !dependency_in_topic && dependent_in_topic,
            HighlightMode::TopicDependents => dependency_in_topic && !dependent_in_topic,
            _ => unreachable!("公开入口已拒绝主题选择的节点模式"),
        };
        if matches {
            result.edges[index] = true;
            result.nodes[edge.dependency] = true;
            result.nodes[edge.dependent] = true;
        }
    }
    result
}
