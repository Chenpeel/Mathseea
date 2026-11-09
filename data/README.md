# 默认导入图

`data/import_graph.txt` 是 MathSeea 无参数启动时使用的默认图, 编译进程序, 不依赖启动目录或相邻仓库.

## 使用

```sh
target/release/mathseea
```

也可以在顶部数据菜单中选择内置图, 或用 `--graph PATH` / `--mathlib PATH` 指定其他数据源. 修改本文件后需要重新构建应用.

文本图格式和源码扫描规则见项目根目录的 [README](../README.md).

## 数据来源

- 来源: [MathlibExplorer](https://github.com/Crispher/MathlibExplorer) 的 `release/data/import_graph.txt`
- 节点数: 3763
- 边数: 10704
- 文件大小: 426128 字节
- SHA-256: `30851bba2edfa9279b16d2555d43dbd69c0510e0129395fd93e6b1859602f464`

原仓库未提供明确的再分发许可证. 当前文件仅作为本地默认数据使用; 对外发布或再分发前请自行核实许可.
