//! MultiTokenManager 子模块组织：core（核心）、model_support（按模型路由）、admin_ops（管理操作）、reports（状态上报）、stats（统计持久化）。
mod admin_ops;
mod core;
mod keep_alive;
mod model_support;
mod reports;
mod stats;

pub use keep_alive::{keep_alive_hours, set_keep_alive_hours};
