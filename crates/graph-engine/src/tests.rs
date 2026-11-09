//! 验收用例仅编写, 本轮不执行测试.

use mathseea_model::{Edge, Highlight, HighlightMode, ImportGraph, ModuleNode, Selection};

use crate::{compute_layout, highlight, topological_order};

fn graph(names: &[&str], pairs: &[(usize, usize)]) -> ImportGraph {
    ImportGraph::new(
        names.iter().map(|name| ModuleNode::new(*name)).collect(),
        pairs
            .iter()
            .map(|&(dependency, dependent)| Edge {
                dependency,
                dependent,
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn topological_order_uses_smallest_ready_id_and_dependency_direction() {
    let graph = graph(&["A", "B", "C", "D"], &[(2, 0), (2, 1)]);
    assert_eq!(topological_order(&graph).unwrap(), [2, 0, 1, 3]);
}

#[test]
fn topological_order_and_compute_layout_report_actual_cycle_names() {
    let graph = graph(&["CycleA", "CycleB", "Blocked"], &[(0, 1), (1, 0), (1, 2)]);
    for error in [
        topological_order(&graph).unwrap_err(),
        compute_layout(&graph).unwrap_err(),
    ] {
        assert!(error.message.contains("CycleA -> CycleB -> CycleA"));
        assert!(!error.message.contains("Blocked"));
    }
}

#[test]
fn compute_layout_is_deterministic_finite_and_strictly_orders_every_edge() {
    let graph = graph(
        &["A.Root", "B.Middle", "A.Leaf", "B.Isolated"],
        &[(0, 1), (1, 2)],
    );
    let layout = compute_layout(&graph).unwrap();
    assert_eq!(layout, compute_layout(&graph).unwrap());
    layout.validate(&graph).unwrap();
    for edge in &graph.edges {
        assert!(layout.nodes[edge.dependency].position.x < layout.nodes[edge.dependent].position.x);
    }
    assert!(layout.nodes[0].radius > layout.nodes[1].radius);
    assert!(layout.nodes[1].radius > layout.nodes[2].radius);
    assert!(layout.nodes[0].position.y < layout.nodes[1].position.y);
    assert!(layout.nodes[2].position.y < layout.nodes[1].position.y);
    assert_eq!(layout.topics.len(), 2);
}

#[test]
fn compute_layout_uses_dependency_count_without_breaking_longer_paths() {
    let graph = graph(
        &["A", "B", "C", "D", "E"],
        &[(0, 2), (1, 2), (2, 3), (3, 4)],
    );
    let layout = compute_layout(&graph).unwrap();
    let xs: Vec<_> = layout.nodes.iter().map(|node| node.position.x).collect();
    assert_eq!(xs, [0.0, 0.0, 200.0, 300.0, 400.0]);
}

fn relation_graph() -> ImportGraph {
    graph(
        &["T.A", "T.B", "U.C", "U.D", "V.E", "T.Isolated"],
        &[(0, 1), (2, 0), (2, 3), (1, 3), (3, 4), (0, 4), (0, 3)],
    )
}

fn assert_highlight(
    graph: &ImportGraph,
    actual: Highlight,
    expected_nodes: &[usize],
    expected_edges: &[(usize, usize)],
) {
    assert!(actual.active);
    assert_eq!(
        actual.nodes,
        (0..graph.nodes.len())
            .map(|id| expected_nodes.contains(&id))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        actual.edges,
        graph
            .edges
            .iter()
            .map(|edge| expected_edges.contains(&(edge.dependency, edge.dependent)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn highlight_neighbors_only_marks_seed_incident_edges() {
    let graph = relation_graph();
    assert_highlight(
        &graph,
        highlight(&graph, &Selection::Node(0), HighlightMode::Neighbors).unwrap(),
        &[0, 1, 2, 3, 4],
        &[(0, 1), (2, 0), (0, 4), (0, 3)],
    );
}

#[test]
fn highlight_dependencies_and_dependents_follow_opposite_transitive_directions() {
    let graph = relation_graph();
    assert_highlight(
        &graph,
        highlight(&graph, &Selection::Node(1), HighlightMode::Dependencies).unwrap(),
        &[0, 1, 2],
        &[(0, 1), (2, 0)],
    );
    assert_highlight(
        &graph,
        highlight(&graph, &Selection::Node(1), HighlightMode::Dependents).unwrap(),
        &[1, 3, 4],
        &[(1, 3), (3, 4)],
    );
}

#[test]
fn highlight_topic_modes_preserve_all_seeds_and_only_matching_edges() {
    let graph = relation_graph();
    let selection = Selection::Topic("T".to_owned());
    for (mode, nodes, edges) in [
        (HighlightMode::TopicMembers, vec![0, 1, 5], vec![(0, 1)]),
        (
            HighlightMode::TopicDependencies,
            vec![0, 1, 2, 5],
            vec![(2, 0)],
        ),
        (
            HighlightMode::TopicDependents,
            vec![0, 1, 3, 4, 5],
            vec![(1, 3), (0, 3), (0, 4)],
        ),
    ] {
        assert_highlight(
            &graph,
            highlight(&graph, &selection, mode).unwrap(),
            &nodes,
            &edges,
        );
    }
}

#[test]
fn highlight_rejects_invalid_selections_and_mode_mismatches() {
    let graph = relation_graph();
    for (selection, mode) in [
        (Selection::Node(99), HighlightMode::Neighbors),
        (Selection::Node(0), HighlightMode::TopicMembers),
        (
            Selection::Topic("Missing".into()),
            HighlightMode::TopicMembers,
        ),
        (Selection::Topic("T".into()), HighlightMode::Neighbors),
    ] {
        assert!(highlight(&graph, &selection, mode).is_err());
    }
    for mode in [HighlightMode::Neighbors, HighlightMode::TopicDependencies] {
        assert_eq!(
            highlight(&graph, &Selection::None, mode).unwrap(),
            Highlight::all(&graph)
        );
    }
}

#[test]
fn all_public_entries_reject_inconsistent_adjacency() {
    let mut graph = relation_graph();
    graph.dependencies[0].clear();
    assert!(topological_order(&graph).is_err());
    assert!(compute_layout(&graph).is_err());
    assert!(highlight(&graph, &Selection::None, HighlightMode::Neighbors).is_err());
}

#[test]
fn empty_graph_and_isolated_seed_are_valid() {
    let empty = graph(&[], &[]);
    assert!(topological_order(&empty).unwrap().is_empty());
    let layout = compute_layout(&empty).unwrap();
    assert!(layout.nodes.is_empty());
    layout.validate(&empty).unwrap();
    let isolated = graph(&["Single"], &[]);
    for mode in [
        HighlightMode::Neighbors,
        HighlightMode::Dependencies,
        HighlightMode::Dependents,
    ] {
        assert_highlight(
            &isolated,
            highlight(&isolated, &Selection::Node(0), mode).unwrap(),
            &[0],
            &[],
        );
    }
}

#[test]
fn highlight_transitive_traversal_terminates_even_for_cycles() {
    let graph = graph(&["A", "B"], &[(0, 1), (1, 0)]);
    assert_highlight(
        &graph,
        highlight(&graph, &Selection::Node(0), HighlightMode::Dependencies).unwrap(),
        &[0, 1],
        &[(0, 1), (1, 0)],
    );
}
