// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 通用数据模型：应用配置、命令行参数、API Key、用量统计/分页/定价、失败日志、地理信息、RPM 与节流日志等结构定义。

pub mod api_key;
pub mod arg;
pub mod chat_session;
pub mod config;
pub mod failure_log;
pub mod geo;
pub mod rpm;
pub mod throttle_log;
pub mod usage;
