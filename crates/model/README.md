# mathseea-model

MathSeea 的公共数据模型, 定义模块、依赖边、布局、选择和高亮掩码.

## 数据约定

- `NodeId` 是节点数组索引.
- `Edge { dependency, dependent }` 表示依赖项指向使用者. 若 B 导入 A, 则记录 A -> B.
- `ImportGraph` 保存节点、连边、直接依赖和直接使用者.
- `ImportGraph::new` 会拒绝空名称、重名、越界索引、重复边和自循环, 并规范化邻接表.
- `Layout` 保存每个节点的位置、半径、主题标签和边界.
- `Selection`, `HighlightMode` 和 `Highlight` 表示当前选择和关系高亮.

## 使用

```rust
use mathseea_model::{Edge, ImportGraph, ModuleNode};

let graph = ImportGraph::new(
    vec![
        ModuleNode::new("Mathlib.A"),
        ModuleNode::new("Mathlib.B"),
    ],
    vec![Edge { dependency: 0, dependent: 1 }],
)?;
```

`ModuleNode::new` 会从 `Mathlib.<Topic>...` 名称提取主题并设置默认主题色. 读取或构建完成后应保持图和布局不可变; 修改布局或选择后, 由视图层负责刷新相机和缓存.

`ImportGraph::validate` 和 `Layout::validate` 会检查外部构造的数据. 它们会创建规范化副本, 不适合在每一帧重复调用.

## 构建

```sh
cargo check -p mathseea-model --locked
```

本 crate 不读写文件, 不依赖图形库.
