# mathseea-graph-engine

对 `mathseea-model` 中的导入图执行拓扑分析、布局和关系高亮. 所有输入都按 `dependency -> dependent` 解释.

## 接口

```rust
pub fn topological_order(graph: &ImportGraph) -> Result<Vec<NodeId>, GraphError>;
pub fn compute_layout(graph: &ImportGraph) -> Result<Layout, GraphError>;
pub fn highlight(
    graph: &ImportGraph,
    selection: &Selection,
    mode: HighlightMode,
) -> Result<Highlight, GraphError>;
```

## 拓扑和布局

- 拓扑顺序保证依赖项出现在使用者之前; 同时就绪的节点按 `NodeId` 排序.
- 遇到循环依赖时返回包含真实闭环模块名的错误.
- 布局按依赖深度分列, 按主题纵向分带. 每条边都满足 `x(dependency) < x(dependent)`.
- 节点半径由反向依赖 PageRank 计算, 结果只取决于输入图和固定参数.
- 所有坐标、半径和边界都必须是有限值.

空图会返回空节点、空主题和 `(0,0)..(1,1)` 边界.

## 高亮模式

节点选择:

| 模式 | 高亮节点 | 高亮边 |
| --- | --- | --- |
| `Neighbors` | 直接依赖和直接使用者 | 与选中节点相接的边 |
| `Dependencies` | 全部传递依赖 | 闭包内部的边 |
| `Dependents` | 全部传递使用者 | 闭包内部的边 |

主题选择:

| 模式 | 高亮节点 | 高亮边 |
| --- | --- | --- |
| `TopicMembers` | 主题内节点 | 主题内部边 |
| `TopicDependencies` | 主题内节点和一跳外部依赖 | 外部依赖指向主题的边 |
| `TopicDependents` | 主题内节点和一跳外部使用者 | 主题指向外部使用者的边 |

`Selection::None` 返回全亮结果. 非法节点 ID、不存在的主题或选择与模式不匹配会返回错误.

## 构建

```sh
cargo check -p mathseea-graph-engine --locked
cargo clippy -p mathseea-graph-engine --locked -- -D warnings
```
