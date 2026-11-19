//! 失败 / 客户端取消请求的明细记录。
//!
//! 正常完成的请求由 `UsageTracker::record` 入库并计入日汇总；失败与客户端取消只写入明细
//! （供使用日志展示），不计入日汇总、费用、credits 与请求数：这两类请求没有可靠的计费数据
//! （上游 meteringEvent 在流结束时才到达），按零 token 记录，避免统计口径被半截数据污染。
//!
//! `RequestGuard` 把「请求没有走到终点」这件事交给 Drop 判定：处理器在入口创建 guard，
//! 正常完成时 `settle()`，已知失败时 `fail()`；其余情况（连接被客户端断开导致 handler
//! future / SSE 流被丢弃）在 Drop 里记为客户端取消。

use chrono::Utc;
use std::sync::Arc;
use std::time::Instant;

use super::{UsageRecord, UsageTracker};

/// 请求失败（上游报错、流中断、空响应、超时等）
pub const OUTCOME_FAILED: &str = "failed";
/// 客户端在响应完成前主动断开
pub const OUTCOME_CLIENT_ABORTED: &str = "client_aborted";

/// 失败原因的最大保留字符数（按字符截断，避免切断多字节字符）
const MAX_ERROR_MESSAGE_CHARS: usize = 300;

/// `record_outcome` 的可选入参（与 `RecordOptions` 同构，避免位置参数膨胀）
///
/// token、费用恒为 0、credits 恒为 None，故不在此列；`first_token_ms` 仅流式请求在已有
/// 首字后才有值。
#[derive(Debug, Clone, Default)]
pub struct OutcomeOptions {
    pub error_message: Option<String>,
    pub credential_id: Option<u64>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub effort: Option<String>,
    pub is_stream: bool,
    pub first_token_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

fn truncate_message(message: &str) -> String {
    let mut chars = message.trim().chars();
    let head: String = chars.by_ref().take(MAX_ERROR_MESSAGE_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

impl UsageTracker {
    /// 记录一条失败 / 客户端取消的请求（只写明细，不动日汇总）
    pub fn record_outcome(
        &self,
        api_key_id: u32,
        model: String,
        outcome: &str,
        options: OutcomeOptions,
    ) {
        let OutcomeOptions {
            error_message,
            credential_id,
            client_ip,
            user_agent,
            effort,
            is_stream,
            first_token_ms,
            duration_ms,
        } = options;
        self.push_record(UsageRecord {
            api_key_id,
            credential_id,
            model,
            input_tokens: 0,
            output_tokens: 0,
            estimated_cost: 0.0,
            credits_used: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
            created_at: Utc::now(),
            client_ip,
            effort,
            first_token_ms,
            duration_ms,
            thinking_token_count: None,
            is_stream: Some(is_stream),
            user_agent,
            outcome: Some(outcome.to_string()),
            error_message: error_message.as_deref().map(truncate_message),
        });
    }
}

/// 一次进行中请求的结果守卫（见模块文档）
pub struct RequestGuard {
    tracker: Arc<UsageTracker>,
    api_key_id: u32,
    model: String,
    is_stream: bool,
    started_at: Instant,
    client_ip: Option<String>,
    user_agent: Option<String>,
    effort: Option<String>,
    credential_id: Option<u64>,
    first_token_ms: Option<u64>,
    settled: bool,
}

impl RequestGuard {
    /// 未启用用量追踪或没有 API Key 上下文时返回 None（此时不记录任何结果）
    pub fn new(
        tracker: Option<&Arc<UsageTracker>>,
        api_key_id: Option<u32>,
        model: &str,
        is_stream: bool,
        started_at: Instant,
        client_ip: Option<String>,
        user_agent: Option<String>,
    ) -> Option<Self> {
        Some(Self {
            tracker: Arc::clone(tracker?),
            api_key_id: api_key_id?,
            model: model.to_string(),
            is_stream,
            started_at,
            client_ip,
            user_agent,
            effort: None,
            credential_id: None,
            first_token_ms: None,
            settled: false,
        })
    }

    /// 上游账号确定后补充（故障转移后的最终账号）
    pub fn set_credential_id(&mut self, credential_id: u64) {
        self.credential_id = Some(credential_id);
    }

    /// 生效的 effort 确定后补充
    pub fn set_effort(&mut self, effort: Option<String>) {
        self.effort = effort;
    }

    /// 记录首个客户端可见输出的时刻（幂等，仅首次生效）
    pub fn mark_first_token(&mut self) {
        if self.first_token_ms.is_none() {
            self.first_token_ms = Some(self.started_at.elapsed().as_millis() as u64);
        }
    }

    /// 请求已正常完成并由 `UsageTracker::record` 入库：解除守卫，Drop 时不再记录
    pub fn settle(&mut self) {
        self.settled = true;
    }

    /// 请求失败：立即记录失败原因并解除守卫
    pub fn fail(&mut self, message: &str) {
        self.record(OUTCOME_FAILED, Some(message));
    }

    fn record(&mut self, outcome: &str, message: Option<&str>) {
        if self.settled {
            return;
        }
        self.settled = true;
        self.tracker.record_outcome(
            self.api_key_id,
            self.model.clone(),
            outcome,
            OutcomeOptions {
                error_message: message.map(str::to_string),
                credential_id: self.credential_id,
                client_ip: self.client_ip.take(),
                user_agent: self.user_agent.take(),
                effort: self.effort.take(),
                is_stream: self.is_stream,
                first_token_ms: self.first_token_ms,
                duration_ms: Some(self.started_at.elapsed().as_millis() as u64),
            },
        );
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        // 走到这里仍未结算：处理器被取消或流被丢弃，即客户端在响应完成前断开
        self.record(OUTCOME_CLIENT_ABORTED, None);
    }
}
