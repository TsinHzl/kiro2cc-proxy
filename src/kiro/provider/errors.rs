//! 错误分类与退避：重试/限流退避延迟、月度限额与 Profile ARN 错误判定、请求体改写。
use std::time::Duration;
use tokio::time::sleep;

use crate::anthropic::converter::native_thinking_supported;
use crate::kiro::model::credentials::{KiroCredentials, fallback_profile_arn_value};
use crate::kiro::model::requests::conversation::HistoryAssistantMessage;

use super::core::KiroProvider;

const THROTTLE_BASE_MS: u64 = 2000;
const THROTTLE_STEP_MS: u64 = 1000;
const THROTTLE_MAX_MS: u64 = 8_000;
const THROTTLE_JITTER_MAX_MS: u64 = 1500;
const RETRY_BACKOFF_MAX_EXPONENT: usize = 6;
const RPM_GATE_DEFAULT_WAIT_SECS: u64 = 3;
const RPM_GATE_MAX_WAIT_SECS: u64 = 5;

impl KiroProvider {
    pub(crate) fn retry_delay(attempt: usize) -> Duration {
        // 指数退避 + 少量抖动，避免上游抖动时放大故障
        const BASE_MS: u64 = 200;
        const MAX_MS: u64 = 5_000;
        let exp = BASE_MS
            .saturating_mul(2u64.saturating_pow(attempt.min(RETRY_BACKOFF_MAX_EXPONENT) as u32));
        let backoff = exp.min(MAX_MS);
        let jitter_max = (backoff / 4).max(1);
        let jitter = fastrand::u64(0..=jitter_max);
        Duration::from_millis(backoff.saturating_add(jitter))
    }

    /// 429 限流退避：随 attempt 递增，避免固定间隔反复命中同一限流窗口
    pub(crate) fn throttle_delay(attempt: usize) -> Duration {
        // 2s + attempt×1s（上限 8s）+ jitter
        let base =
            THROTTLE_BASE_MS.saturating_add((attempt as u64).saturating_mul(THROTTLE_STEP_MS));
        let capped = base.min(THROTTLE_MAX_MS);
        let jitter = fastrand::u64(0..=THROTTLE_JITTER_MAX_MS);
        Duration::from_millis(capped.saturating_add(jitter))
    }

    /// RPM 硬限制：精确等待到下一个 slot 释放（上限 5s，短兜底避免长尾阻塞）
    ///
    /// 返回 true 表示等待后已有 slot 可用或本身未满；返回 false 表示等待超时仍满
    pub(crate) async fn wait_for_rpm_gate(&self, credential_id: u64, tag: &str) -> bool {
        let Some(rpm) = &self.rpm_tracker else {
            return true;
        };
        let max_rpm = self.token_manager.max_rpm_per_credential();
        if max_rpm == 0 || rpm.credential_rpm(credential_id) < max_rpm as u64 {
            return true;
        }

        // 精确计算等待时间
        let wait_duration = rpm
            .time_until_slot(credential_id, max_rpm)
            .unwrap_or(Duration::from_secs(RPM_GATE_DEFAULT_WAIT_SECS))
            .min(Duration::from_secs(RPM_GATE_MAX_WAIT_SECS));

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

    /// 单次解析管线：profileArn 改写 + 账号级 thinking 开关判定合并处理，
    /// 避免大请求体（Claude Code 场景可达数 MB）在链路内被多轮 parse/serialize。
    ///
    /// - JSON 解析失败 → 原样返回，不阻断请求
    /// - `thinking_adaptive_requested`：客户端请求了 adaptive thinking（与 converter
    ///   注入层判定一致，enabled 不计入），当前未被本函数消费（thinking 是否生效仅由
    ///   账号开关决定），保留参数以避免 retry.rs 透传链联动改动
    /// - MCP 路径复用（`rewrite_profile_arn`）传 false；MCP 请求体无 `modelId`，
    ///   不会触发 thinking 注入
    /// - 账号开关 `thinkingAdaptive` 是 thinking 的最终裁决：关闭 = 剥离（即使客户端
    ///   请求了）；开启 = 对支持的模型强制注入（即使客户端未请求）
    pub(crate) fn rewrite_request_body(
        body: &str,
        credentials: &KiroCredentials,
        _thinking_adaptive_requested: bool,
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

        // 账号级开关判定：converter 层（fields.rs）已按请求/模型完成注入决策，
        // 此处按实际选中账号决定"该账号是否保留 thinking 字段"——
        // `thinkingAdaptive == false` 时剥离（含 converter 未注入的情形，无副作用），
        // 故障转移后按新账号重判，每次重试都以当次实际选中的账号状态为准。
        // （旧模型类型判定 GPT 系 / "4.5" 代际已上移至 converter 注入层，不再重复。）
        let fields = obj.get_mut("additionalModelRequestFields");
        if let Some(fields) = fields.and_then(|f| f.as_object_mut())
            && fields.contains_key("thinking")
            && !credentials.thinking_adaptive
        {
            fields.remove("thinking");
        }

        // 账号开关是 thinking 的最终裁决，与客户端是否请求无关：
        // - 关闭：除原生字段外，一并剥离 converter 为 `enabled` 请求注入到 history[0]
        //   的 `<thinking_mode>` 文本标签，否则 4.5 代际等走文本标签协议的请求会无视
        //   开关继续深度思考；响应侧同步丢弃上游推理内容（见 handlers）。
        // - 开启：客户端未请求 thinking 时，对支持原生字段的模型强制注入 adaptive。
        if !credentials.thinking_adaptive {
            Self::strip_text_thinking_tag(&mut value);
        } else {
            Self::inject_native_thinking(&mut value);
        }

        serde_json::to_string(&value).unwrap_or_else(|_| body.to_string())
    }

    /// 账号开关开启时强制注入原生 `thinking: {"type": "adaptive"}`。
    ///
    /// 仅在同时满足以下条件时注入，其余情形原样返回：
    /// - 请求体 `modelId` 属于支持该字段的模型（[`native_thinking_supported`]；
    ///   GPT 系 / "4.5" 代际 / 第三方模型会被上游 400 拒绝，不能注入）；
    /// - `additionalModelRequestFields` 尚无 `thinking`（客户端已请求 adaptive 时
    ///   converter 已注入，不重复）；
    /// - history[0] 不含 converter 注入的 `<thinking_mode>` 文本标签前缀（`enabled` 请求已由文本标签协议
    ///   控制 thinking，再叠加原生字段会出现两套互相独立的控制信号，v3.4.1 曾因此
    ///   导致客户端规则遵从性回退）。
    fn inject_native_thinking(value: &mut serde_json::Value) {
        let supported = value
            .pointer("/conversationState/currentMessage/userInputMessage/modelId")
            .and_then(|m| m.as_str())
            .is_some_and(native_thinking_supported);
        if !supported {
            return;
        }
        // 仅识别 converter 注入的精确前缀（与剥离共用判定），普通正文里引用标签字面量
        // 不算——否则账号开启却既无文本标签也无原生字段，控制信号全部丢失
        let has_text_tag = value
            .pointer("/conversationState/history")
            .and_then(|h| h.as_array())
            .is_some_and(|h| Self::injected_text_tag_remainder(h).is_some());
        if has_text_tag {
            return;
        }
        let Some(obj) = value.as_object_mut() else {
            return;
        };
        let fields = obj
            .entry("additionalModelRequestFields")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(fields) = fields.as_object_mut() {
            fields
                .entry("thinking")
                .or_insert_with(|| serde_json::json!({ "type": "adaptive" }));
        }
    }

    /// 剥离 converter 注入在 history[0] 最前面的 thinking 文本标签。
    ///
    /// 仅匹配 converter 生成的精确形态
    /// `<thinking_mode>enabled</thinking_mode><max_thinking_length>N</max_thinking_length>`，
    /// 且要求紧随其后的 history[1] 是 converter 配对插入的固定确认语
    /// （[`HistoryAssistantMessage::SYSTEM_ACK`]）且不含工具调用——
    /// 否则视为用户原文，整段历史不动（避免误删真实消息、破坏 tool_use/tool_result 配对）：
    /// - 标签后跟 `\n` + 系统提示 → 只去掉标签与换行，保留系统提示；
    /// - 标签单独成条（无系统消息时 converter 插入的 user + ack 配对）→ 整对移除，
    ///   避免留下空 content（上游会拒绝）。
    ///
    /// 其他位置、其他形态的标签（如客户端自带）一律不动。
    /// 识别 converter 注入在 history[0] 的 thinking 文本标签前缀。
    ///
    /// 命中（精确形态 + history[1] 为配对确认语且无工具调用）时返回去掉标签（及其后
    /// 一个换行）后的剩余内容；否则返回 `None`，视为用户原文。
    fn injected_text_tag_remainder(history: &[serde_json::Value]) -> Option<String> {
        const OPEN: &str = "<thinking_mode>enabled</thinking_mode><max_thinking_length>";
        const CLOSE: &str = "</max_thinking_length>";

        let content = history
            .first()?
            .pointer("/userInputMessage/content")?
            .as_str()?;
        let rest = content.strip_prefix(OPEN)?;
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        let rest = rest[digits..].strip_prefix(CLOSE)?;
        let remainder = rest.strip_prefix('\n').unwrap_or(rest).to_string();

        // 来源校验：converter 注入的前缀恒与固定确认语配对；不匹配（或带工具调用）说明
        // 这条 user 消息是用户原文，不能删也不能改
        let is_injected_pair = history.get(1).is_some_and(|m| {
            m.pointer("/assistantResponseMessage/content")
                .and_then(|c| c.as_str())
                == Some(HistoryAssistantMessage::SYSTEM_ACK)
                && m.pointer("/assistantResponseMessage/toolUses")
                    .and_then(|t| t.as_array())
                    .is_none_or(|t| t.is_empty())
        });
        is_injected_pair.then_some(remainder)
    }

    fn strip_text_thinking_tag(value: &mut serde_json::Value) {
        let Some(history) = value
            .pointer_mut("/conversationState/history")
            .and_then(|h| h.as_array_mut())
        else {
            return;
        };
        let Some(remainder) = Self::injected_text_tag_remainder(history) else {
            return;
        };

        if remainder.is_empty() {
            // 单独成条：移除 user + 紧随其后的确认语 assistant 配对
            history.drain(..2);
        } else if let Some(c) = history[0].pointer_mut("/userInputMessage/content") {
            *c = serde_json::Value::String(remainder);
        }
    }
}
