# mathseea-graph-view

在 egui 中绘制导入图. 提供 2D 拓扑视图和 3D 星云视图, 两者共用图模型、选中状态和高亮掩码.

## 2D 视图

```rust
use mathseea_graph_view::{GraphView, ViewEvent, ViewOptions};

let mut view = GraphView::default();
let events: Vec<ViewEvent> = view.show(
    ui,
    &graph,
    &layout,
    &highlight,
    &selection,
    &ViewOptions::default(),
);
```

- 滚轮锚定光标缩放.
- 左键或中键拖拽平移.
- 点击节点、主题或空白返回 `ViewEvent`.
- `reset_camera()` 重新适配视图.
- `focus_node(id)` 居中指定节点.

## 3D 星云

```rust
use mathseea_graph_view::NebulaView;

let render_state = context.wgpu_render_state.as_ref().unwrap();
let mut nebula = NebulaView::new(render_state);
let events = nebula.show(
    ui,
    &graph,
    &layout,
    &highlight,
    &selection,
    &ViewOptions::default(),
);
```

- 左键拖拽旋转, Shift+左键、中键或右键拖拽平移, 滚轮缩放.
- `reset_camera()` 复位, `focus_node(id)` 居中指定节点.
- X 轴保持拓扑顺序; 旋转后的屏幕左右不再代表依赖方向.
- 选中关系的箭头绘制在两球之间的有效边段内.
- 拾取使用最近可见球面, 标签只显示有限的局部候选.

3D 复用应用的 eframe wgpu 设备, 使用内置 WGSL 着色器. 没有可用设备时会返回空事件, 由应用显示诊断; 2D 视图不受影响.

## 生命周期

载入新图或原位修改图、布局后, 调用 `GraphView::reset_camera()` 和 `NebulaView::reset_document()` 使缓存失效. 文档载入后应保持不可变.

`ViewOptions` 控制连边、主题提示和节点标签. `ViewEvent` 只表达选择意图, 视图不会直接修改模型.

## 构建

```sh
cargo check -p mathseea-graph-view --locked
cargo clippy -p mathseea-graph-view --locked -- -D warnings
```
