use std::ffi::OsString;
use std::path::PathBuf;

use mathseea_model::GraphError;

pub const HELP: &str = "MathSeea - Mathlib 导入图\n\n用法:\n  mathseea                         打开内置的默认文本图\n  mathseea --graph PATH            打开原格式文本图\n  mathseea --mathlib PATH          扫描本地 mathlib4 或 Mathlib 目录\n  mathseea --mathlib PATH --export PATH\n                                  计算布局并导出, 不打开窗口\n  mathseea --graph PATH --export PATH\n                                  读取并导出文本图, 不打开窗口\n  mathseea --help                  显示帮助\n\n图文件与源码目录二选一. 路径包含空格时请加引号.\n导出目标必须是新文件. 应用不调用 Lean, Lake, Python 或其他外部程序.\n";

#[derive(Clone, Debug)]
pub enum Source {
    Bundled,
    Graph(PathBuf),
    Mathlib(PathBuf),
}

impl Source {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Bundled => std::path::Path::new("data/import_graph.txt"),
            Self::Graph(path) | Self::Mathlib(path) => path,
        }
    }
}

#[derive(Debug)]
pub struct Cli {
    pub source: Option<Source>,
    pub export: Option<PathBuf>,
    pub help: bool,
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, GraphError> {
        let mut args = args.into_iter();
        let mut result = Self {
            source: None,
            export: None,
            help: false,
        };
        while let Some(argument) = args.next() {
            match argument.to_str() {
                Some("--help" | "-h") => result.help = true,
                Some(flag @ ("--graph" | "--mathlib" | "--export")) => {
                    let value = args
                        .next()
                        .filter(|value| {
                            !value.is_empty() && !value.to_string_lossy().starts_with("--")
                        })
                        .ok_or_else(|| GraphError::new(format!("{flag} 缺少路径参数")))?;
                    let path = PathBuf::from(value);
                    if flag == "--export" {
                        if result.export.replace(path).is_some() {
                            return Err(GraphError::new("--export 只能指定一次"));
                        }
                    } else {
                        if result.source.is_some() {
                            return Err(GraphError::new(
                                "--graph 与 --mathlib 只能选择其中一个, 且只能指定一次",
                            ));
                        }
                        result.source = Some(if flag == "--graph" {
                            Source::Graph(path)
                        } else {
                            Source::Mathlib(path)
                        });
                    }
                }
                _ => {
                    return Err(GraphError::new(format!(
                        "未知参数: {}. 使用 --help 查看用法",
                        argument.to_string_lossy()
                    )));
                }
            }
        }
        if !result.help && result.export.is_some() && result.source.is_none() {
            return Err(GraphError::new(
                "--export 必须同时指定 --graph 或 --mathlib",
            ));
        }
        Ok(result)
    }
}
