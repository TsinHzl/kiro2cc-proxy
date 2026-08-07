// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 更新日志静态数据
//!
//! 与 `build_model_list()`（`src/anthropic/handlers.rs`）同模式：编译期硬编码，
//! 随代码发布同步维护。新增版本时在 `build_release_notes()` 顶部追加一条，
//! 并将上一条的 `is_latest` 改回 `false`。

/// 中英双语文案
#[derive(Debug, Clone)]
pub struct Bilingual {
    pub zh: String,
    pub en: String,
}

impl Bilingual {
    fn new(zh: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            zh: zh.into(),
            en: en.into(),
        }
    }
}

/// 更新日志分类分组（固定使用「新功能」「优化」「修复」三类）
#[derive(Debug, Clone)]
pub struct ReleaseNoteGroup {
    pub title: Bilingual,
    pub items: Vec<Bilingual>,
}

/// 单个版本的更新日志
#[derive(Debug, Clone)]
pub struct ReleaseNote {
    pub version: String,
    pub date: String,
    pub is_latest: bool,
    pub groups: Vec<ReleaseNoteGroup>,
}

fn feat_group(items: Vec<Bilingual>) -> ReleaseNoteGroup {
    ReleaseNoteGroup {
        title: Bilingual::new("新功能", "New Features"),
        items,
    }
}

fn improve_group(items: Vec<Bilingual>) -> ReleaseNoteGroup {
    ReleaseNoteGroup {
        title: Bilingual::new("优化", "Improvements"),
        items,
    }
}

fn fix_group(items: Vec<Bilingual>) -> ReleaseNoteGroup {
    ReleaseNoteGroup {
        title: Bilingual::new("修复", "Fixes"),
        items,
    }
}

/// 构建更新日志列表，固定按版本号从新到旧声明（不做运行时排序）
pub fn build_release_notes() -> Vec<ReleaseNote> {
    vec![
        ReleaseNote {
            version: "2.9.0".to_string(),
            date: "2026-08-07".to_string(),
            is_latest: true,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "Admin 后台支持中英文全局切换",
                    "Admin console now supports global zh/en language switching",
                )]),
                improve_group(vec![Bilingual::new(
                    "设置页面重构为分组列表布局",
                    "Settings page redesigned with a grouped list layout",
                )]),
            ],
        },
        ReleaseNote {
            version: "2.8.25".to_string(),
            date: "2026-08-07".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "支持模型列表按模型家族分组排列",
                "Supported models list is now grouped by model family",
            )])],
        },
        ReleaseNote {
            version: "2.8.24".to_string(),
            date: "2026-08-06".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![
                Bilingual::new(
                    "Admin 登录密码字段由 adminApiKey 改名为 adminPsw",
                    "Renamed the admin login password field from adminApiKey to adminPsw",
                ),
                Bilingual::new(
                    "控制台标题链接新增 hover 高亮效果",
                    "Added a hover highlight effect to the console title link",
                ),
            ])],
        },
        ReleaseNote {
            version: "2.8.23".to_string(),
            date: "2026-08-06".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "移除主 API Key 全局兜底认证机制，收窄鉴权入口",
                "Removed the global fallback authentication via the master API key to narrow the authentication surface",
            )])],
        },
        ReleaseNote {
            version: "2.8.22".to_string(),
            date: "2026-08-05".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "支持模型页标记同家族内最低/最高费率模型",
                    "The supported models page now flags the lowest/highest priced model within each family",
                ),
                Bilingual::new(
                    "支持模型页按提供方家族着色",
                    "The supported models page is now color-coded by provider family",
                ),
                Bilingual::new(
                    "按模型分组卡片新增 credits 消费统计",
                    "Added credits consumption stats to model-grouped cards",
                ),
            ])],
        },
        ReleaseNote {
            version: "2.8.21".to_string(),
            date: "2026-08-05".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "每日统计页新增最近 14 天 credits 使用趋势曲线图",
                "Added a 14-day credits usage trend chart to the daily stats page",
            )])],
        },
        ReleaseNote {
            version: "2.8.20".to_string(),
            date: "2026-08-05".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Dockerfile 缺少 COPY assets 导致容器内 ip2region xdb 缺失、构建失败的问题",
                "Fixed a build failure caused by the Dockerfile missing a COPY assets step, which left the ip2region xdb file absent in the container",
            )])],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exactly_one_latest_version() {
        let notes = build_release_notes();
        let latest_count = notes.iter().filter(|n| n.is_latest).count();
        assert_eq!(latest_count, 1, "必须有且仅有一条 is_latest=true");
    }

    #[test]
    fn test_version_format() {
        let notes = build_release_notes();
        for note in &notes {
            assert!(
                note.version.split('.').count() == 3
                    && note
                        .version
                        .split('.')
                        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())),
                "版本号格式非法: {}",
                note.version
            );
        }
    }

    #[test]
    fn test_bilingual_fields_non_empty() {
        let notes = build_release_notes();
        for note in &notes {
            for group in &note.groups {
                assert!(!group.title.zh.is_empty(), "分组标题 zh 不能为空");
                assert!(!group.title.en.is_empty(), "分组标题 en 不能为空");
                for item in &group.items {
                    assert!(!item.zh.is_empty(), "条目 zh 不能为空");
                    assert!(!item.en.is_empty(), "条目 en 不能为空");
                }
            }
        }
    }
}
