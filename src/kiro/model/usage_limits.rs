// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 使用额度查询数据模型
//!
//! 包含 getUsageLimits API 的响应类型定义。
//! 字段口径对齐 Kiro-account-manager 的 `parseUsageResponse`：精确值
//! （`*WithPrecision`）优先于整数值；freeTrial / bonus 仅 `ACTIVE` 计入汇总。

use serde::Deserialize;

/// 使用额度查询响应
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
// 对齐 KAM 完整响应模型的全量镜像：部分字段暂无读取方，保留供后续详情扩展，非遗漏
#[allow(dead_code)]
pub struct UsageLimitsResponse {
    /// 下次重置日期 (Unix 时间戳)
    #[serde(default)]
    pub next_date_reset: Option<f64>,

    /// 订阅信息
    #[serde(default)]
    pub subscription_info: Option<SubscriptionInfo>,

    /// 用户信息（依赖 `isEmailRequired=true` 查询参数返回）
    #[serde(default)]
    pub user_info: Option<UserInfo>,

    /// 超额配置
    #[serde(default)]
    pub overage_configuration: Option<OverageConfiguration>,

    /// 使用量明细列表
    #[serde(default)]
    pub usage_breakdown_list: Vec<UsageBreakdown>,
}

/// 订阅信息
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SubscriptionInfo {
    /// 订阅标题 (KIRO PRO+ / KIRO FREE 等)
    #[serde(default)]
    pub subscription_title: Option<String>,

    /// 原始订阅类型 (Q_DEVELOPER_STANDALONE_PRO 等)。
    /// 上游 REST 响应的真实键名为 `type`（KAM 两处 `rawType` 赋值均读 `.type`）；
    /// `subscriptionType` 为部分上游形态的别名，双向兼容
    #[serde(default, rename = "type", alias = "subscriptionType")]
    pub subscription_type: Option<String>,

    /// 可升级能力 (UPGRADE_CAPABLE 等)
    #[serde(default)]
    pub upgrade_capability: Option<String>,

    /// 超额能力
    #[serde(default)]
    pub overage_capability: Option<String>,
}

/// 用户信息
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct UserInfo {
    /// 用户邮箱
    #[serde(default)]
    pub email: Option<String>,

    /// 用户 ID（d-xxxx 长格式）
    #[serde(default)]
    pub user_id: Option<String>,
}

/// 超额配置
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct OverageConfiguration {
    /// 是否启用超额计费
    #[serde(default)]
    pub overage_enabled: Option<bool>,

    /// 超额状态 (ENABLED / DISABLED)
    #[serde(default)]
    pub overage_status: Option<String>,
}

/// 使用量明细
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct UsageBreakdown {
    /// 资源类型 (CREDIT 等)
    #[serde(default)]
    pub resource_type: Option<String>,

    /// 资源显示名 (Credit)
    #[serde(default)]
    pub display_name: Option<String>,

    /// 资源显示名复数 (Credits)
    #[serde(default)]
    pub display_name_plural: Option<String>,

    /// 币种 (USD)
    #[serde(default)]
    pub currency: Option<String>,

    /// 计费单位 (INVOCATIONS)
    #[serde(default)]
    pub unit: Option<String>,

    /// 超额费率 (0.04)
    #[serde(default)]
    pub overage_rate: Option<f64>,

    /// 超额上限
    #[serde(default)]
    pub overage_cap: Option<f64>,

    /// 当前使用量
    #[serde(default)]
    pub current_usage: i64,

    /// 当前使用量（精确值）
    #[serde(default)]
    pub current_usage_with_precision: f64,

    /// 奖励额度列表
    #[serde(default)]
    pub bonuses: Vec<Bonus>,

    /// 免费试用信息
    #[serde(default)]
    pub free_trial_info: Option<FreeTrialInfo>,

    /// 下次重置日期 (Unix 时间戳)
    #[serde(default)]
    pub next_date_reset: Option<f64>,

    /// 使用限额
    #[serde(default)]
    pub usage_limit: i64,

    /// 使用限额（精确值）
    #[serde(default)]
    pub usage_limit_with_precision: f64,
}

/// 奖励额度
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct Bonus {
    /// 奖励 Code
    #[serde(default)]
    pub bonus_code: Option<String>,

    /// 奖励显示名 (Trial credits 等)
    #[serde(default)]
    pub display_name: Option<String>,

    /// 当前使用量
    #[serde(default)]
    pub current_usage: f64,

    /// 使用限额
    #[serde(default)]
    pub usage_limit: f64,

    /// 过期时间 (Unix 时间戳，秒)
    #[serde(default)]
    pub expires_at: Option<f64>,

    /// 状态 (ACTIVE / EXPIRED)
    #[serde(default)]
    pub status: Option<String>,
}

impl Bonus {
    /// 检查 bonus 是否处于激活状态
    pub fn is_active(&self) -> bool {
        self.status
            .as_deref()
            .map(|s| s == "ACTIVE")
            .unwrap_or(false)
    }
}

/// 免费试用信息
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct FreeTrialInfo {
    /// 当前使用量
    #[serde(default)]
    pub current_usage: i64,

    /// 当前使用量（精确值）
    #[serde(default)]
    pub current_usage_with_precision: f64,

    /// 免费试用过期时间 (Unix 时间戳，秒)
    #[serde(default)]
    pub free_trial_expiry: Option<f64>,

    /// 免费试用状态 (ACTIVE / EXPIRED)
    #[serde(default)]
    pub free_trial_status: Option<String>,

    /// 使用限额
    #[serde(default)]
    pub usage_limit: i64,

    /// 使用限额（精确值）
    #[serde(default)]
    pub usage_limit_with_precision: f64,
}

// ============ 便捷方法实现 ============

impl FreeTrialInfo {
    /// 检查免费试用是否处于激活状态
    pub fn is_active(&self) -> bool {
        self.free_trial_status
            .as_deref()
            .map(|s| s == "ACTIVE")
            .unwrap_or(false)
    }
}

impl UsageLimitsResponse {
    /// 获取订阅标题
    pub fn subscription_title(&self) -> Option<&str> {
        self.subscription_info
            .as_ref()
            .and_then(|info| info.subscription_title.as_deref())
    }

    /// 用于写回账号订阅等级的标题：优先 `subscriptionTitle`，缺失或为空白时回退到
    /// 原始订阅类型 `type`（如 Q_DEVELOPER_STANDALONE_PRO）。
    ///
    /// 部分账号（如企业版 IdC）只返回 `type`；回退后账号才算「订阅已知」，
    /// 且类型串含 FREE 时仍会被 `supports_opus` 识别为 Free。
    pub fn resolved_subscription_title(&self) -> Option<&str> {
        let info = self.subscription_info.as_ref()?;
        [
            info.subscription_title.as_deref(),
            info.subscription_type.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
    }

    /// 获取第一个使用量明细
    fn primary_breakdown(&self) -> Option<&UsageBreakdown> {
        self.usage_breakdown_list.first()
    }

    /// 获取总使用限额（精确值）
    ///
    /// 累加基础额度、激活的免费试用额度和激活的奖励额度
    pub fn usage_limit(&self) -> f64 {
        let Some(breakdown) = self.primary_breakdown() else {
            return 0.0;
        };

        let mut total = breakdown.usage_limit_with_precision;

        // 累加激活的 free trial 额度
        if let Some(trial) = &breakdown.free_trial_info
            && trial.is_active()
        {
            total += trial.usage_limit_with_precision;
        }

        // 累加激活的 bonus 额度
        for bonus in &breakdown.bonuses {
            if bonus.is_active() {
                total += bonus.usage_limit;
            }
        }

        total
    }

    /// 获取总当前使用量（精确值）
    ///
    /// 累加基础使用量、激活的免费试用使用量和激活的奖励使用量
    pub fn current_usage(&self) -> f64 {
        let Some(breakdown) = self.primary_breakdown() else {
            return 0.0;
        };

        let mut total = breakdown.current_usage_with_precision;

        // 累加激活的 free trial 使用量
        if let Some(trial) = &breakdown.free_trial_info
            && trial.is_active()
        {
            total += trial.current_usage_with_precision;
        }

        // 累加激活的 bonus 使用量
        for bonus in &breakdown.bonuses {
            if bonus.is_active() {
                total += bonus.current_usage;
            }
        }

        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// KAM 实测响应形态（REST API）样例；上游真实键名为 `type`，
    /// `subscriptionType` 为兼容别名（serde alias 双向兼容）
    const FULL_RESPONSE: &str = r#"{
        "nextDateReset": 1799136000,
        "subscriptionInfo": {
            "subscriptionTitle": "KIRO PRO",
            "type": "Q_DEVELOPER_STANDALONE_PRO",
            "upgradeCapability": "UPGRADE_CAPABLE",
            "overageCapability": "OVERAGE_CAPABLE"
        },
        "userInfo": {
            "email": "user@example.com",
            "userId": "d-9067c98495-44e8a498-9041-70e1-18f5-00bc4525ad65"
        },
        "overageConfiguration": {
            "overageEnabled": true,
            "overageStatus": "ENABLED"
        },
        "usageBreakdownList": [
            {
                "resourceType": "CREDIT",
                "displayName": "Credit",
                "displayNamePlural": "Credits",
                "currency": "USD",
                "unit": "INVOCATIONS",
                "overageRate": 0.04,
                "overageCap": 10000,
                "currentUsage": 38,
                "currentUsageWithPrecision": 37.6,
                "usageLimit": 1000,
                "usageLimitWithPrecision": 1000,
                "freeTrialInfo": {
                    "currentUsage": 0,
                    "currentUsageWithPrecision": 0,
                    "usageLimit": 0,
                    "usageLimitWithPrecision": 0,
                    "freeTrialStatus": "EXPIRED",
                    "freeTrialExpiry": 1735689600
                },
                "bonuses": [
                    {
                        "bonusCode": "0tcrw2acft3da3mmhed#0",
                        "displayName": "Trial credits",
                        "currentUsage": 0,
                        "currentUsageWithPrecision": 0,
                        "usageLimit": 500,
                        "usageLimitWithPrecision": 500,
                        "expiresAt": 1822568100,
                        "status": "ACTIVE"
                    },
                    {
                        "bonusCode": "expired#1",
                        "displayName": "Old credits",
                        "currentUsage": 12.5,
                        "usageLimit": 100,
                        "expiresAt": 1600000000,
                        "status": "EXPIRED"
                    }
                ]
            }
        ]
    }"#;

    #[test]
    fn deserializes_full_response_with_detail_fields() {
        let resp: UsageLimitsResponse = serde_json::from_str(FULL_RESPONSE).unwrap();

        // 顶层扩展字段
        let user = resp.user_info.as_ref().unwrap();
        assert_eq!(user.email.as_deref(), Some("user@example.com"));
        assert_eq!(
            user.user_id.as_deref(),
            Some("d-9067c98495-44e8a498-9041-70e1-18f5-00bc4525ad65")
        );
        let overage = resp.overage_configuration.as_ref().unwrap();
        assert_eq!(overage.overage_status.as_deref(), Some("ENABLED"));
        assert_eq!(overage.overage_enabled, Some(true));
        assert_eq!(resp.next_date_reset, Some(1799136000.0));

        // 订阅扩展字段（type 键 → subscription_type 字段）
        let sub = resp.subscription_info.as_ref().unwrap();
        assert_eq!(sub.subscription_title.as_deref(), Some("KIRO PRO"));
        assert_eq!(
            sub.subscription_type.as_deref(),
            Some("Q_DEVELOPER_STANDALONE_PRO")
        );
        assert_eq!(sub.upgrade_capability.as_deref(), Some("UPGRADE_CAPABLE"));

        // breakdown 扩展字段
        let b = resp.primary_breakdown().unwrap();
        assert_eq!(b.resource_type.as_deref(), Some("CREDIT"));
        assert_eq!(b.display_name.as_deref(), Some("Credit"));
        assert_eq!(b.currency.as_deref(), Some("USD"));
        assert_eq!(b.unit.as_deref(), Some("INVOCATIONS"));
        assert_eq!(b.overage_rate, Some(0.04));
        assert_eq!(b.overage_cap, Some(10000.0));
    }

    #[test]
    fn deserializes_bonus_detail_fields() {
        let resp: UsageLimitsResponse = serde_json::from_str(FULL_RESPONSE).unwrap();
        let b = resp.primary_breakdown().unwrap();

        assert_eq!(b.bonuses.len(), 2);
        let active = &b.bonuses[0];
        assert_eq!(active.bonus_code.as_deref(), Some("0tcrw2acft3da3mmhed#0"));
        assert_eq!(active.display_name.as_deref(), Some("Trial credits"));
        assert_eq!(active.expires_at, Some(1822568100.0));
        assert!(active.is_active());

        let expired = &b.bonuses[1];
        assert!(!expired.is_active());
        assert_eq!(expired.current_usage, 12.5);
    }

    #[test]
    fn free_trial_active_status_and_expiry() {
        let resp: UsageLimitsResponse = serde_json::from_str(FULL_RESPONSE).unwrap();
        let trial = resp
            .primary_breakdown()
            .unwrap()
            .free_trial_info
            .as_ref()
            .unwrap();

        // 样例中 freeTrial 为 EXPIRED：不入汇总、不计激活
        assert!(!trial.is_active());
        assert_eq!(trial.free_trial_expiry, Some(1735689600.0));

        // 汇总 = 基础 1000 + ACTIVE bonus 500（EXPIRED 不计、freeTrial 不计）
        assert_eq!(resp.usage_limit(), 1500.0);
        assert_eq!(resp.current_usage(), 37.6);
    }

    #[test]
    fn free_trial_active_counts_into_totals() {
        let resp: UsageLimitsResponse = serde_json::from_str(
            r#"{"usageBreakdownList":[{
                "usageLimitWithPrecision": 1000,
                "currentUsageWithPrecision": 10,
                "freeTrialInfo": {
                    "usageLimitWithPrecision": 50,
                    "currentUsageWithPrecision": 5,
                    "freeTrialStatus": "ACTIVE"
                }
            }]}"#,
        )
        .unwrap();
        assert_eq!(resp.usage_limit(), 1050.0);
        assert_eq!(resp.current_usage(), 15.0);
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        // 旧上游形态 / 字段缺失：全部 Option + default，不反序列化失败
        let resp: UsageLimitsResponse =
            serde_json::from_str(r#"{"usageBreakdownList":[{"usageLimit":100}]}"#).unwrap();
        assert!(resp.user_info.is_none());
        assert!(resp.overage_configuration.is_none());
        assert!(resp.subscription_info.is_none());
        let b = resp.primary_breakdown().unwrap();
        assert!(b.resource_type.is_none());
        assert!(b.overage_rate.is_none());
        assert!(b.free_trial_info.is_none());
        assert!(b.bonuses.is_empty());
        assert_eq!(b.usage_limit, 100);
    }

    #[test]
    fn tolerates_empty_response() {
        let resp: UsageLimitsResponse = serde_json::from_str("{}").unwrap();
        assert_eq!(resp.usage_limit(), 0.0);
        assert_eq!(resp.current_usage(), 0.0);
        assert!(resp.subscription_title().is_none());
    }

    #[test]
    fn resolved_subscription_title_prefers_title_then_falls_back_to_type() {
        let cases = [
            // (subscriptionInfo JSON, 期望)
            (
                r#"{"subscriptionInfo": {"subscriptionTitle": "KIRO PRO", "type": "Q_DEVELOPER_STANDALONE_PRO"}}"#,
                Some("KIRO PRO"),
            ),
            (
                r#"{"subscriptionInfo": {"subscriptionTitle": "   ", "type": "Q_DEVELOPER_STANDALONE_PRO"}}"#,
                Some("Q_DEVELOPER_STANDALONE_PRO"),
            ),
            (
                r#"{"subscriptionInfo": {"type": "Q_DEVELOPER_STANDALONE_FREE"}}"#,
                Some("Q_DEVELOPER_STANDALONE_FREE"),
            ),
            (
                r#"{"subscriptionInfo": {"subscriptionTitle": "", "type": " "}}"#,
                None,
            ),
            (r#"{"subscriptionInfo": {}}"#, None),
            ("{}", None),
        ];
        for (json, expected) in cases {
            let resp: UsageLimitsResponse = serde_json::from_str(json).unwrap();
            assert_eq!(resp.resolved_subscription_title(), expected, "json={json}");
        }
    }

    #[test]
    fn subscription_title_getter_does_not_fall_back_to_type() {
        // 余额弹窗仍展示原始标题：回退只作用于账号等级写回
        let resp: UsageLimitsResponse =
            serde_json::from_str(r#"{"subscriptionInfo": {"type": "Q_DEVELOPER_STANDALONE_PRO"}}"#)
                .unwrap();
        assert!(resp.subscription_title().is_none());
    }

    #[test]
    fn subscription_type_accepts_subscription_type_alias() {
        // 部分上游 / KAM 防御性声明用 subscriptionType 键名，alias 双向兼容
        let resp: UsageLimitsResponse = serde_json::from_str(
            r#"{"subscriptionInfo": {"subscriptionType": "Q_DEVELOPER_STANDALONE_PRO"}}"#,
        )
        .unwrap();
        assert_eq!(
            resp.subscription_info.unwrap().subscription_type.as_deref(),
            Some("Q_DEVELOPER_STANDALONE_PRO")
        );
    }
}
