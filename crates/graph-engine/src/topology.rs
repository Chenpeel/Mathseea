use std::cmp::Reverse;
use std::collections::BinaryHeap;

use mathseea_model::{GraphError, ImportGraph, NodeId};

/// 返回依赖项在使用者之前的拓扑顺序; 同时就绪时优先选择较小 NodeId.
pub fn topological_order(graph: &ImportGraph) -> Result<Vec<NodeId>, GraphError> {
    graph.validate()?;
    validated_order(graph)
}

pub(crate) fn validated_order(graph: &ImportGraph) -> Result<Vec<NodeId>, GraphError> {
    let mut remaining: Vec<_> = graph.dependencies.iter().map(Vec::len).collect();
    let mut ready = BinaryHeap::new();
    for (id, &count) in remaining.iter().enumerate() {
        if count == 0 {
            ready.push(Reverse(id));
        }
    }
    let mut order = Vec::with_capacity(graph.nodes.len());
    while let Some(Reverse(dependency)) = ready.pop() {
        order.push(dependency);
        for &dependent in &graph.dependents[dependency] {
            remaining[dependent] -= 1;
            if remaining[dependent] == 0 {
                ready.push(Reverse(dependent));
            }
        }
    }
    if order.len() != graph.nodes.len() {
        return Err(cycle_error(graph, &remaining));
    }
    Ok(order)
}

// 只报告真实闭环, 不将被循环阻塞的下游模块误报为循环成员.
// 显式栈避免长依赖链耗尽调用栈.
fn cycle_error(graph: &ImportGraph, remaining: &[usize]) -> GraphError {
    let mut state = vec![0_u8; graph.nodes.len()];
    let mut path_positions = vec![0; graph.nodes.len()];
    let mut stack = Vec::new();
    for start in 0..graph.nodes.len() {
        if remaining[start] == 0 || state[start] != 0 {
            continue;
        }
        state[start] = 1;
        path_positions[start] = 0;
        stack.push((start, 0));
        while let Some((node, next_index)) = stack.pop() {
            if next_index == graph.dependents[node].len() {
                state[node] = 2;
                continue;
            }
            stack.push((node, next_index + 1));
            let dependent = graph.dependents[node][next_index];
            if remaining[dependent] == 0 || state[dependent] == 2 {
                continue;
            }
            if state[dependent] == 1 {
                let mut names: Vec<_> = stack[path_positions[dependent]..]
                    .iter()
                    .map(|&(id, _)| graph.nodes[id].name.as_str())
                    .collect();
                names.push(graph.nodes[dependent].name.as_str());
                return GraphError::new(format!("循环依赖: {}", names.join(" -> ")));
            }
            state[dependent] = 1;
            path_positions[dependent] = stack.len();
            stack.push((dependent, 0));
        }
    }
    GraphError::new("拓扑排序失败: 图包含循环依赖")
}
