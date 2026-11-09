//! 进程内的原文本图 IO 与 Mathlib 文件头扫描; 不启动 Lean 或其他外部程序.

mod lean;
mod legacy;

pub use lean::scan_mathlib;
pub use legacy::{export_legacy, parse_legacy, read_legacy, write_legacy};
