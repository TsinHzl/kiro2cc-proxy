// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin API 模块
//!
//! 提供账号管理和监控功能的 HTTP API
//!
//! # 功能
//! - 查询所有账号状态
//! - 启用/禁用账号
//! - 修改账号优先级
//! - 重置失败计数
//! - 查询账号余额
//! - API Key 多用户管理
//!
//! # 使用
//! ```ignore
//! let admin_service = AdminService::new(token_manager.clone());
//! let admin_state = AdminState::new(admin_api_key, admin_service);
//! let admin_router = create_admin_router(admin_state);
//! ```

mod api_keys;
mod error;
mod handlers;
mod middleware;
mod router;
mod service;
pub mod types;

pub use middleware::AdminState;
pub use router::create_admin_router;
pub use service::AdminService;
