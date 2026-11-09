//! 纯进程内的确定性图分析、布局与关系高亮.
//! 边始终从 dependency 指向 dependent, 所有公开入口均校验图模型.

mod highlight;
mod layout;
mod topology;

pub use highlight::highlight;
pub use layout::compute_layout;
pub use topology::topological_order;

#[cfg(test)]
mod tests;
