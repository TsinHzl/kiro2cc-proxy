//! 错误分类与请求体改写：退避延迟、月度限额/Profile ARN 判定、请求体改写

use std::time::Duration;
use tokio::time::sleep;

use crate::kiro::model::credentials::{KiroCredentials, fallback_profile_arn_value};

use super::core::KiroProvider;

impl KiroProvider {
    pub(crate) fn retry_delay(attempt: usize) -> Duration {
        // 指数退避 + 少量抖动，避免上游抖动时放大故障
        const BASE_MS: u64 = 200;
        const MAX_MS: u64 = 5_000;
        let exp = BASE_MS.saturating_mul(2u64.saturating_pow(attempt.min(6) as u32));
        let backoff = exp.min(MAX_MS);
        let jitter_max = (backoff / 4).max(1);
        let jitter = fastrand::u64(0..=jitter_max);
        Duration::from_millis(backoff.saturating_add(jitter))
    }

    /// 429 限流退避：随 attempt 递增，避免固定间隔反复命中同一限流窗口
    pub(crate) fn throttle_delay(attempt: usize) -> Duration {
        // 2s + attempt×1s（上限 8s）+ jitter
        let base = 2000u64.saturating_add((attempt as u64).saturating_mul(1000));
        let capped = base.min(8_000);
        let jitter = fastrand::u64(0..=1500);
        Duration::from_millis(capped.saturating_add(jitter))
    }

    /// RPM 硬限制：精确等待到下一个 slot 释放（上限 5s，短兜底避免长尾阻塞）
    ///
    /// 返回 true 表示等待后已有 slot 可用或本身未满；返回 false 表示等待超时仍满
    pub(crate) async fn wait_for_rpm_gate(&self, credential_id: u64, tag: &str) -> bool {
        let Some(rpm) = &self.rpm_tracker else {
            return true;
        };
        let max_rpm = self.token_manager.config().max_rpm_per_credential;
        if max_rpm == 0 || rpm.credential_rpm(credential_id) < max_rpm as u64 {
            return true;
        }

        // 精确计算等待时间
        let wait_duration = rpm
            .time_until_slot(credential_id, max_rpm)
            .unwrap_or(Duration::from_secs(3))
            .min(Duration::from_secs(5));

        tracing::info!(
            "[RPM-GATE] credential={} rpm={} limit={}, waiting {:.1}s{}",
            credential_id,
            rpm.credential_rpm(credential_id),
            max_rpm,
            wait_duration.as_secs_f64(),
            tag
        );
        sleep(wait_duration).await;

        if rpm.credential_rpm(credential_id) >= max_rpm as u64 {
            tracing::warn!(
                "[RPM-GATE] credential={} still over limit after wait{}, proceeding anyway",
                credential_id,
                tag
            );
            return false;
        }
        true
    }

    pub(crate) fn is_monthly_request_limit(body: &str) -> bool {
        if body.contains("MONTHLY_REQUEST_COUNT") {
            return true;
        }

        let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
            return false;
        };

        if value
            .get("reason")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == "MONTHLY_REQUEST_COUNT")
        {
            return true;
        }

        value
            .pointer("/error/reason")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == "MONTHLY_REQUEST_COUNT")
    }

    /// 检测数据面 400 是否为"账号缺少 profileArn"类错误。
    ///
    /// 企业 IdC 账号必须携带 profileArn，缺失时上游返回
    /// `400 {"message":"profileArn is required for this request."}`，
    /// 属账号级缺陷而非请求级错误，应故障转移到其他账号。
    pub(crate) fn is_profile_arn_required_error(body: &str) -> bool {
        body.contains("profileArn is required")
    }

    /// 无 profile_arn 账号的数据面 fallback ARN。
    ///
    /// 常量与推导逻辑统一定义在 [`crate::kiro::model::credentials`]（添加/加载账号时
    /// 即按此补全并持久化），此处委托保持行为一致。
    pub(crate) fn fallback_profile_arn(credentials: &KiroCredentials) -> Option<&'static str> {
        fallback_profile_arn_value(credentials)
    }

    /// 将请求 body 中的 `profileArn` 替换为当前选中账号的值。
    ///
    /// - 账号有 profile_arn → 设置 / 覆盖字段
    /// - 账号无 profile_arn → 按账号类型注入固定 ARN（见 [`Self::fallback_profile_arn`]）：
    ///   上游数据面对所有账号都要求该字段存在，缺失会 400 "profileArn is required"
    /// - JSON 解析失败 → 原样返回，不阻断请求
    pub(crate) fn rewrite_profile_arn(body: &str, credentials: &KiroCredentials) -> String {
        Self::rewrite_request_body(body, credentials, false)
    }

    /// 单次解析管线：profileArn 改写 + 按需 thinking adaptive 注入合并处理，
    /// 避免大请求体（Claude Code 场景可达数 MB）在链路内被多轮 parse/serialize。
    ///
    /// - JSON 解析失败 → 原样返回，不阻断请求
    /// - `requested=false` 时跳过注入，仅做 profileArn 改写（含 MCP 路径复用）
    pub(crate) fn rewrite_request_body(
        body: &str,
        credentials: &KiroCredentials,
        thinking_adaptive_requested: bool,
    ) -> String {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(body) else {
            return body.to_string();
        };
        let obj = match value.as_object_mut() {
            Some(o) => o,
            None => return body.to_string(),
        };
        let arn = match &credentials.profile_arn {
            Some(arn) => Some(arn.as_str()),
            None => Self::fallback_profile_arn(credentials),
        };
        match arn {
            Some(arn) => {
                obj.insert(
                    "profileArn".to_string(),
                    serde_json::Value::String(arn.to_string()),
                );
            }
            None => {
                obj.remove("profileArn");
            }
        }

        // 按账号级开关注入 `additionalModelRequestFields.thinking`，复用同一份
        // 已解析的 value，不产生第二次 parse/serialize。注入条件（全部满足）：
        // - `thinking_adaptive_requested` 为 true（客户端请求了 adaptive）
        // - 账号级开关 `credentials.thinking_adaptive` 已开启
        // - 目标模型非 GPT 系且非 "4.5" 代际（复用 converter 侧
        //   `additional_fields_skipped` 谓词，与 `build_additional_model_request_fields`
        //   的整体跳过条件保持单一来源；modelId 取不到时 fail-closed 跳过）
        if thinking_adaptive_requested && credentials.thinking_adaptive {
            let model_id = obj
                .get("conversationState")
                .and_then(|cs| cs.get("currentMessage"))
                .and_then(|cm| cm.get("userInputMessage"))
                .and_then(|uim| uim.get("modelId"))
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let model_id = model_id.as_str();
            if !model_id.is_empty()
                && !crate::anthropic::converter::additional_fields_skipped(model_id)
                && !crate::anthropic::converter::is_gpt_model(model_id)
            {
                let fields = obj
                    .entry("additionalModelRequestFields")
                    .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                match fields.as_object_mut() {
                    Some(f) => {
                        f.insert(
                            "thinking".to_string(),
                            serde_json::json!({ "type": "adaptive" }),
                        );
                        tracing::debug!(
                            "[THINKING-ADAPTIVE] injected: credential={} model_id={} model_type=adaptive",
                            credentials.id.map(|i| i.to_string()).unwrap_or_default(),
                            model_id
                        );
                    }
                    None => tracing::warn!(
                        "[THINKING-ADAPTIVE] additionalModelRequestFields 非对象，跳过注入: credential={} model_id={}",
                        credentials.id.map(|i| i.to_string()).unwrap_or_default(),
                        model_id
                    ),
                }
            }
        }

        serde_json::to_string(&value).unwrap_or_else(|_| body.to_string())
    }
}
