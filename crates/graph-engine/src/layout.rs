use std::collections::BTreeMap;

use mathseea_model::{Bounds, GraphError, ImportGraph, Layout, NodePlacement, Point, TopicLabel};

use crate::topology::validated_order;

const COLUMN_SPACING: f64 = 100.0;
const ROW_SPACING: f64 = 36.0;
const TOPIC_GAP: f64 = 72.0;
const MIN_RADIUS: f64 = 4.0;
const MAX_RADIUS: f64 = 14.0;
const DAMPING: f64 = 0.85;
const PAGERANK_ITERATIONS: usize = 100;
const PAGERANK_TOLERANCE: f64 = 1e-10;

/// 按依赖计数/最长依赖路径分列, 主题分带, 反向依赖 PageRank 决定半径.
pub fn compute_layout(graph: &ImportGraph) -> Result<Layout, GraphError> {
    graph.validate()?;
    let order = validated_order(graph)?;
    if graph.is_empty() {
        return Ok(Layout {
            nodes: Vec::new(),
            topics: Vec::new(),
            bounds: Bounds {
                min: Point::new(0.0, 0.0),
                max: Point::new(1.0, 1.0),
            },
        });
    }

    let mut columns: Vec<_> = graph.dependencies.iter().map(Vec::len).collect();
    let mut bands: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for &id in &order {
        for &dependency in &graph.dependencies[id] {
            columns[id] = columns[id].max(columns[dependency] + 1);
        }
        bands.entry(&graph.nodes[id].topic).or_default().push(id);
    }
    let ranks = reverse_pagerank(graph);
    let max_rank = ranks.iter().copied().fold(0.0_f64, f64::max);
    let mut nodes = vec![
        NodePlacement {
            position: Point::default(),
            radius: MIN_RADIUS as f32,
        };
        graph.nodes.len()
    ];
    let mut topics = Vec::with_capacity(bands.len());
    let mut band_top = 0.0;
    for (topic, mut ids) in bands {
        ids.sort_unstable_by_key(|&id| (columns[id], id));
        let mut previous_column = None;
        let mut column_rows = 0;
        let mut max_rows = 0;
        for &id in &ids {
            // 不同横列复用纵向槽位, 主题带高度只由最拥挤列决定.
            if previous_column != Some(columns[id]) {
                previous_column = Some(columns[id]);
                column_rows = 0;
            }
            nodes[id] = NodePlacement {
                position: Point::new(
                    (columns[id] as f64 * COLUMN_SPACING) as f32,
                    (band_top + column_rows as f64 * ROW_SPACING) as f32,
                ),
                radius: (MIN_RADIUS + (MAX_RADIUS - MIN_RADIUS) * (ranks[id] / max_rank).sqrt())
                    as f32,
            };
            column_rows += 1;
            max_rows = max_rows.max(column_rows);
        }
        let band_height = (max_rows - 1) as f64 * ROW_SPACING;
        let middle = ids.len() / 2;
        topics.push(TopicLabel {
            topic: topic.to_owned(),
            position: Point::new(
                nodes[ids[middle]].position.x,
                (band_top + band_height / 2.0) as f32,
            ),
            color: graph.nodes[ids[0]].color,
        });
        band_top += band_height + TOPIC_GAP;
    }

    let mut min = Point::new(f32::INFINITY, f32::INFINITY);
    let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for node in &nodes {
        if !node.position.x.is_finite()
            || !node.position.y.is_finite()
            || !node.radius.is_finite()
            || node.radius <= 0.0
        {
            return Err(GraphError::new("布局超出有限坐标或半径范围"));
        }
        min.x = min.x.min(node.position.x - node.radius);
        min.y = min.y.min(node.position.y - node.radius);
        max.x = max.x.max(node.position.x + node.radius);
        max.y = max.y.max(node.position.y + node.radius);
    }
    if !min.x.is_finite() || !min.y.is_finite() || !max.x.is_finite() || !max.y.is_finite() {
        return Err(GraphError::new("布局边界超出有限坐标范围"));
    }
    if graph
        .edges
        .iter()
        .any(|edge| nodes[edge.dependency].position.x >= nodes[edge.dependent].position.x)
    {
        return Err(GraphError::new(
            "布局坐标精度不足以保持依赖项严格位于使用者左侧",
        ));
    }
    if topics
        .iter()
        .any(|label| !label.position.x.is_finite() || !label.position.y.is_finite())
    {
        return Err(GraphError::new("布局主题标签超出有限坐标范围"));
    }
    Ok(Layout {
        nodes,
        topics,
        bounds: Bounds { min, max },
    })
}

// 将使用者的权重沿导入方向传给依赖项, 而不是沿绘图边传给使用者.
// 无依赖节点的悬挂质量均匀分配; 固定上限和 L1 收敛条件保证可复现.
fn reverse_pagerank(graph: &ImportGraph) -> Vec<f64> {
    let count = graph.nodes.len();
    let count_f64 = count as f64;
    let mut ranks = vec![1.0 / count_f64; count];
    let mut next = vec![0.0; count];
    for _ in 0..PAGERANK_ITERATIONS {
        let dangling: f64 = graph
            .dependencies
            .iter()
            .zip(&ranks)
            .filter(|(dependencies, _)| dependencies.is_empty())
            .map(|(_, rank)| rank)
            .sum();
        next.fill((1.0 - DAMPING + DAMPING * dangling) / count_f64);
        for (dependent, dependencies) in graph.dependencies.iter().enumerate() {
            if dependencies.is_empty() {
                continue;
            }
            let contribution = DAMPING * ranks[dependent] / dependencies.len() as f64;
            for &dependency in dependencies {
                next[dependency] += contribution;
            }
        }
        let difference: f64 = ranks
            .iter()
            .zip(&next)
            .map(|(before, after)| (before - after).abs())
            .sum();
        std::mem::swap(&mut ranks, &mut next);
        if difference <= PAGERANK_TOLERANCE {
            break;
        }
    }
    ranks
}
