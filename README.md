# MathSeea

MathSeea 是一个用 Rust 编写的 Mathlib 导入图桌面查看器. 它读取文本图或扫描本地 mathlib4 源码, 用 2D 拓扑图和可旋转的 3D 星云视图展示模块、主题与依赖关系. 解析、布局和绘制均在应用进程内完成.

## 功能

- 2D 拓扑图: 依赖项始终位于使用者左侧, 支持缩放、平移、节点和主题选择.
- 3D 星云视图: 可旋转、缩放和复位, 支持节点拾取、关系箭头和局部标签.
- 搜索: 按模块名过滤, 结果列表可滚动, 点击后定位到对应节点.
- 主题导航和模块检查器: 查看主题、直接依赖、直接使用者和关系高亮.
- 导出: 将当前图和 2D 布局写入新的文本文件.
- 默认数据: 无参数启动时打开内置的 `data/import_graph.txt`.

应用不启动 Lean、Lake、Python、Graphviz、浏览器或其他外部程序.

## 环境要求

- Rust 1.95 或更新版本.
- Linux 桌面环境, X11 或 Wayland 会话.
- 支持 wgpu 的图形驱动.
- 中文界面需要系统中的 Noto Sans CJK 或文泉驿字体.

## 构建

```sh
cargo build --release --locked
```

构建产物为 `target/release/mathseea`.

## 运行

```sh
# 打开内置默认图
target/release/mathseea

# 打开其他文本图
target/release/mathseea --graph /path/to/import_graph.txt

# 扫描 mathlib4 源码目录, 也可传入其 Mathlib 子目录
target/release/mathseea --mathlib /path/to/mathlib4

# 只导出, 不创建窗口
target/release/mathseea --mathlib /path/to/mathlib4 --export /path/to/new-graph.txt
target/release/mathseea --graph /path/to/graph.txt --export /path/to/new-graph.txt

# 查看帮助
target/release/mathseea --help
```

`--graph` 与 `--mathlib` 二选一. `--export` 必须与其中一项一起使用, 目标必须是尚不存在的新文件. 导出模式不初始化图形窗口.

## 界面操作

### 2D 拓扑

- 滚轮: 以光标为中心缩放.
- 左键或中键拖拽: 平移画布.
- 点击节点: 查看直接邻居、传递依赖和传递使用者.
- 点击主题标签: 查看主题成员、主题依赖和引用主题的模块.
- 点击空白: 清除选择.

### 3D 星云

- 左键拖拽: 旋转视角.
- Shift+左键、中键或右键拖拽: 平移.
- 滚轮: 缩放.
- 点击节点: 选择最近的可见球面.
- X 轴表示拓扑方向. 旋转后屏幕左右不再严格对应依赖方向, 视口会显示方向提示和轴标.

### 工作台

- 顶部命令栏: 搜索、2D/3D 切换、数据加载、导出和适配视图.
- 左侧主题导航: 筛选主题并切换选择.
- 右侧检查器: 查看模块、关系模式、直接依赖和使用者.
- 底部状态栏: 显示任务状态、错误和操作提示.
- Ctrl+K: 聚焦搜索框.
- 窄窗口下可收起导航, 检查器会以浮动窗口打开.

## 数据格式

文本图第一行是节点数量 N. 后续 N 行每行为:

```text
模块名 #RRGGBB x y radius
```

最后 N 行每行为:

```text
dependency_id dependent_count dependent_id...
```

邻接行表示依赖项指向使用者. 若模块 B 导入模块 A, 数据中记录 A -> B. 应用会检查节点数量、索引、重复记录、有限坐标和正半径, 并拒绝循环依赖及不满足从左到右布局的图.

## 源码扫描

`--mathlib` 只读取指定 `Mathlib/` 树中的 `.lean` 文件, 不执行 Lean 编译或证明检查.

- 解析文件头的 `module`, `prelude` 和 `import` 声明.
- 支持 `public`, `meta`, `private`, `all` 等导入修饰符.
- 忽略注释和字符串中的伪导入.
- 只把 Mathlib 内部模块建成节点; Lean、Batteries 等外部导入会显示为说明.
- 不跟随符号链接, 不读取 `.olean`, 不补齐隐式依赖.

这是静态头部扫描器, 不保证覆盖任意 Lean 语法扩展. 多模块导入的续行需要缩进.

## 3D 说明

3D 坐标由现有布局派生: X 保持拓扑顺序, Y 和 Z 用于空间展开. 3D 不会改变节点 ID、原始 2D 布局或导出文件. 3D 使用 eframe 现有的 wgpu 设备和内置着色器, 没有额外渲染引擎或外部素材.

如果当前运行环境无法提供 wgpu 设备, 3D 视图会显示错误提示, 2D 视图仍可正常使用.

## 项目结构

```text
crates/model        公共图、布局和选择类型
crates/graph-io     文本图读写和 Lean 导入扫描
crates/graph-engine 拓扑、布局和关系高亮
crates/graph-view   2D/3D 绘制、相机、拾取和选择事件
crates/app          CLI、桌面界面和数据加载
data                默认文本图
```
