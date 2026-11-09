# mathseea 应用

MathSeea 的桌面应用入口, 负责命令行参数、数据加载、工作台界面和导出.

## 运行

```sh
# 无参数启动, 加载内置 data/import_graph.txt
cargo run --release --locked

# 打开文本图
cargo run --release --locked -- --graph /path/to/import_graph.txt

# 扫描 mathlib4
cargo run --release --locked -- --mathlib /path/to/mathlib4

# 导出到新文件, 不打开窗口
cargo run --release --locked -- --graph /path/to/graph.txt --export /path/to/new.txt

cargo run --release --locked -- --help
```

也可以先构建 `cargo build --release --locked`, 再运行 `target/release/mathseea`.

`--graph` 和 `--mathlib` 二选一. `--export` 必须同时指定输入, 目标文件必须不存在. 路径包含空格时请加引号; 应用不会展开 `~` 或环境变量.

## 界面

- 顶部: 搜索、2D/3D 切换、数据、导出、适配视图.
- 左侧: 主题导航和筛选.
- 中央: 2D 拓扑或 3D 星云.
- 右侧: 模块检查器、关系模式和显示选项.
- 底部: 加载状态、错误和操作提示.
- Ctrl+K: 聚焦搜索.
- 窄窗口下可隐藏导航, 检查器会变成浮动窗口.

2D 中滚轮缩放, 左键或中键平移. 3D 中左键旋转, Shift+左键、中键或右键平移, 滚轮缩放. 两个视图共享选择、搜索和高亮状态.

## 源码结构

```text
src/main.rs     CLI/GUI 分流和窗口创建
src/cli.rs      命令行参数
src/document.rs 数据加载、布局和导出
src/app.rs      工作台、后台任务和选择状态
src/theme.rs    深色主题和控件样式
```

加载和导出在后台线程执行, 同时最多一个文件任务. 导出只写入文本图和 2D 布局, 不包含 3D 相机状态.

## 数据来源

无参数启动使用编译进程序的 `data/import_graph.txt`. 修改该文件后需要重新构建应用. 也可以在顶部数据菜单中重新加载内置图, 或用 `--graph`/`--mathlib` 覆盖默认来源.

应用不调用 Lean、Lake、Python、浏览器或其他外部程序. 中文字符使用系统字体; 没有可用中文字体时, 中文界面可能显示不完整.
