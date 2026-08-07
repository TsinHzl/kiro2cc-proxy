// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 更新日志查询处理器

use axum::{Json, response::IntoResponse};

use super::{
    changelog_data::build_release_notes,
    types::{
        AdminBilingualText, AdminReleaseNote, AdminReleaseNoteGroup, AdminReleaseNotesResponse,
    },
};

/// GET /api/admin/changelog
/// 获取更新日志列表（按版本号从新到旧降序排列）
pub async fn get_changelog() -> impl IntoResponse {
    let data = build_release_notes()
        .into_iter()
        .map(|note| AdminReleaseNote {
            version: note.version,
            date: note.date,
            is_latest: note.is_latest,
            groups: note
                .groups
                .into_iter()
                .map(|group| AdminReleaseNoteGroup {
                    title_zh: group.title.zh,
                    title_en: group.title.en,
                    items: group
                        .items
                        .into_iter()
                        .map(|item| AdminBilingualText {
                            zh: item.zh,
                            en: item.en,
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();

    Json(AdminReleaseNotesResponse {
        object: "list".to_string(),
        data,
    })
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[tokio::test]
    async fn test_get_changelog_returns_sorted_list_with_single_latest() {
        let response = get_changelog().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("读取响应体失败");
        let parsed: AdminReleaseNotesResponse =
            serde_json::from_slice(&body).expect("解析响应体失败");

        assert_eq!(parsed.object, "list");
        assert_eq!(parsed.data.len(), 6);
        assert_eq!(parsed.data.iter().filter(|n| n.is_latest).count(), 1);

        let versions: Vec<&str> = parsed.data.iter().map(|n| n.version.as_str()).collect();
        let mut sorted = versions.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(versions, sorted, "版本号必须按降序排列");
        assert_eq!(versions[0], "2.8.25");
        assert!(parsed.data[0].is_latest);
    }
}
