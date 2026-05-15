//! User API 模块
//!
//! 提供用户端用量查询功能的 HTTP API
//!
//! # 功能
//! - 用户通过 API Key 登录
//! - 查询自己的用量数据

mod handlers;
mod middleware;
mod router;

pub use middleware::UserState;
pub use router::create_user_router;
