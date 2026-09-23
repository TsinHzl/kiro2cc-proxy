//! 重试与故障转移：select_endpoint / call_api_with_retry

use tokio::time::sleep;

use crate::kiro::endpoint::{BUCKET_THROTTLE_DURATION, Endpoint, EndpointName};
use crate::kiro::model::credentials::KiroCredentials;

use super::core::{KiroProvider, MAX_RETRIES_PER_CREDENTIAL, MAX_TOTAL_RETRIES};

impl KiroProvider {
    pub(crate) fn select_endpoint(
        &self,
        credentials: &KiroCredentials,
        attempt: usize,
    ) -> Option<Endpoint> {
        let region = credentials.effective_api_region(self.token_manager.config());
        let endpoints = credentials.effective_endpoints(region);
        if endpoints.is_empty() {
            return None;
        }
        let len = endpoints.len();
        // 起点 attempt 偏移（attempt % len），轮询保证单账号内多次 attempt 走不同端点
        let start = attempt % len;
        for i in 0..len {
            let candidate = &endpoints[(start + i) % len];
            if !self
                .endpoint_registry
                .is_throttled(credentials.id.unwrap_or(0), candidate.name)
            {
                return Some(candidate.clone());
            }
        }
        None
    }

    /// 内部方法：带重试逻辑的 API 调用
    ///
    /// 重试策略：
    /// - 每个账号最多重试 MAX_RETRIES_PER_CREDENTIAL 次
    /// - 总重试次数 = min(可用池大小 × 每账号重试次数, MAX_TOTAL_RETRIES)
    /// - 硬上限 9 次，避免无限重试
    /// - 当可用池 ≤ 1 时，仅重试 3 次并使用更长退避间隔
    /// - 单账号内 3 次 attempts 之间切换多端点（不消耗切账号配额）
    pub(crate) async fn call_api_with_retry(
        &self,
        request_body: &str,
        is_stream: bool,
        is_compact: bool,
        thinking_adaptive_requested: bool,
        bound_ids: &[u64],
    ) -> anyhow::Result<(reqwest::Response, u64)> {
        let _permit = self.concurrency_limit.acquire().await?;
        // 流式请求的响应体在上游 200 headers 返回后仍持续生成，受 reqwest
        // 总超时约束，统一使用 `UPSTREAM_TIMEOUT_SECS` 长超时档；若使用较短
        // 档位会在长生成中途被掐断（历史分档背景见常量注释）
        let use_long_timeout = is_stream || is_compact;
        let effective_pool = if bound_ids.is_empty() {
            self.token_manager.total_count()
        } else {
            bound_ids.len()
        };
        let max_retries = (effective_pool * MAX_RETRIES_PER_CREDENTIAL).min(MAX_TOTAL_RETRIES);
        let small_pool = effective_pool <= 1;
        let mut last_error: Option<anyhow::Error> = None;
        let api_type = if is_stream { "流式" } else { "非流式" };

        // 尝试从请求体中提取模型信息和会话 ID
        let model = Self::extract_model_from_request(request_body);
        let continuation_id = Self::extract_continuation_id_from_request(request_body);
        // 本次请求内已限流的账号：重试时避开，但不销毁 sticky 绑定
        let mut throttled_in_request: Vec<u64> = Vec::new();

        for attempt in 0..max_retries {
            // 获取调用上下文（优先路由到同一会话的缓存账号）
            let ctx = match self
                .token_manager
                .acquire_context_sticky(
                    model.as_deref(),
                    bound_ids,
                    continuation_id.as_deref(),
                    &throttled_in_request,
                )
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };

            // RPM 硬限制：精确等待或多账号时 skip（先于并发 permit 获取，避免等待期间占用账号级并发槛位）
            let rpm_ok = self.wait_for_rpm_gate(ctx.id, "").await;
            if !rpm_ok && !small_pool {
                tracing::info!("[RPM-GATE] credential={} RPM 满，跳过切换下一账号", ctx.id);
                self.token_manager.report_throttled_for_rotation(ctx.id);
                if let Some(cid) = continuation_id.as_deref() {
                    self.token_manager.report_sticky_throttled(cid, ctx.id);
                }
                if !throttled_in_request.contains(&ctx.id) {
                    throttled_in_request.push(ctx.id);
                }
                last_error = Some(anyhow::anyhow!(
                    "RPM limit exceeded for credential {}",
                    ctx.id
                ));
                continue;
            }

            // 获取单账号并发 permit（非 sleep 的 continue/Err 分支依赖 _cred_permit 随作用域结束隐式释放；
            // 仅进入 sleep 退避的分支需要显式 drop，以便退避等待期间归还槛位）
            let _cred_permit = self.semaphore_for(ctx.id).acquire_owned().await?;

            // 选择下一个可用端点（单账号内轮询，跳过被封禁桶）
            let endpoint = match self.select_endpoint(&ctx.credentials, attempt) {
                Some(e) => e,
                None => {
                    // 4 桶全封：该账号本次请求内已无可用端点。
                    // 登记避让后继续重试其它账号，仅在所有账号都走到这一步时才失败，
                    // 避免多账号池里因单账号端点全封而直接返回 502。
                    let endpoints: Vec<EndpointName> = ctx
                        .credentials
                        .effective_endpoints(
                            ctx.credentials
                                .effective_api_region(self.token_manager.config()),
                        )
                        .iter()
                        .map(|e| e.name)
                        .collect();
                    let ids = endpoints
                        .iter()
                        .map(|n| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    tracing::info!(
                        "[ENDPOINT] credential={} 端点全封（{}），避让并尝试其它账号",
                        ctx.id,
                        ids
                    );
                    if !throttled_in_request.contains(&ctx.id) {
                        throttled_in_request.push(ctx.id);
                    }
                    last_error = Some(anyhow::anyhow!(
                        "All endpoints throttled for credential {} (tried: [{}])",
                        ctx.id,
                        ids
                    ));
                    // 所有候选账号都已端点全封时立刻换号是空转，需要退避等待桶解封。
                    // 单账号池同理。桶封禁窗口固定，退避比连续空转更快拿到可用端点。
                    let all_avoided = self
                        .token_manager
                        .credential_ids()
                        .iter()
                        .all(|id| throttled_in_request.contains(id));
                    if (small_pool || all_avoided) && attempt + 1 < max_retries {
                        drop(_cred_permit);
                        sleep(Self::throttle_delay(attempt)).await;
                    }
                    continue;
                }
            };
            tracing::debug!(
                "[ENDPOINT] credential={} attempt={} selected={:?} host={}",
                ctx.id,
                attempt + 1,
                endpoint.name,
                endpoint.host
            );

            let url = self.base_url_for(&ctx.credentials, &endpoint);
            let effective_body = Self::rewrite_request_body(
                request_body,
                &ctx.credentials,
                thinking_adaptive_requested,
            );
            let headers = match self.build_headers(&ctx, &effective_body, attempt, &endpoint) {
                Ok(h) => h,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };

            // 发送请求
            let client = match self.client_for(&ctx.credentials, use_long_timeout) {
                Ok(c) => c,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            tracing::debug!("[KIRO-REQUEST] url={} body={}", url, effective_body);
            let response = match client
                .post(&url)
                .headers(headers)
                .body(effective_body)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::warn!(
                        "API 请求发送失败（尝试 {}/{}）: {}",
                        attempt + 1,
                        max_retries,
                        e
                    );
                    last_error = Some(e.into());
                    if attempt + 1 < max_retries {
                        drop(_cred_permit);
                        sleep(Self::retry_delay(attempt)).await;
                    }
                    continue;
                }
            };

            let status = response.status();

            // 成功响应
            if status.is_success() {
                self.token_manager.report_success(ctx.id);
                if let Some(rpm) = &self.rpm_tracker {
                    rpm.record_credential(ctx.id);
                }
                return Ok((response, ctx.id));
            }

            // 失败响应：读取 body 用于日志/错误信息
            let body = response.text().await.unwrap_or_default();

            // 402 Payment Required 且额度用尽：禁用账号并故障转移
            if status.as_u16() == 402 && Self::is_monthly_request_limit(&body) {
                tracing::warn!(
                    "API 请求失败（额度已用尽，禁用账号并切换，尝试 {}/{}）: {} {}",
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );

                let has_available = self.token_manager.report_quota_exhausted(ctx.id);
                if !has_available {
                    // 全局无可用账号：必须走 describe_unavailable 产出带
                    // QUOTA_EXHAUSTED_ALL_MARKER 的文案，否则这条错误串在
                    // handlers.rs 会被误判进 502 兜底，而非正确的 402。
                    anyhow::bail!(
                        "{}",
                        self.token_manager
                            .describe_unavailable(model.as_deref(), bound_ids)
                    );
                }

                last_error = Some(anyhow::anyhow!(
                    "{} API 请求失败: {} {}",
                    api_type,
                    status,
                    body
                ));
                continue;
            }

            // 400 Bad Request - 请求问题，重试/切换账号无意义；
            // 例外：企业 IdC 账号缺失 profileArn 属确定性配置缺陷——首即禁用
            // （ProfileArnMissing，不做失败计数）并故障转移到其他账号
            if status.as_u16() == 400 {
                if Self::is_profile_arn_required_error(&body) {
                    tracing::warn!(
                        "API 请求失败（账号缺少 profileArn，尝试 {}/{}）: {} {}",
                        attempt + 1,
                        max_retries,
                        status,
                        body
                    );
                    // 确定性配置缺陷：首即禁用（ProfileArnMissing），不做失败计数
                    let has_available = self.token_manager.report_profile_arn_missing(ctx.id);
                    if let Some(ref store) = self.failure_log_store {
                        store.record(ctx.id, "api", status.as_u16(), &body);
                    }
                    if !has_available {
                        anyhow::bail!(
                            "{} API 请求失败（所有账号已用尽）: {} {}",
                            api_type,
                            status,
                            body
                        );
                    }
                    last_error = Some(anyhow::anyhow!(
                        "{} API 请求失败: {} {}",
                        api_type,
                        status,
                        body
                    ));
                    continue;
                }
                anyhow::bail!("{} API 请求失败: {} {}", api_type, status, body);
            }

            // 401/403 - 更可能是账号/权限问题：计入失败并允许故障转移
            if matches!(status.as_u16(), 401 | 403) {
                tracing::warn!(
                    "API 请求失败（可能为账号错误，尝试 {}/{}）: {} {}",
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );

                let has_available = self.token_manager.report_failure(ctx.id);
                if let Some(ref store) = self.failure_log_store {
                    store.record(ctx.id, "api", status.as_u16(), &body);
                }
                if !has_available {
                    anyhow::bail!(
                        "{} API 请求失败（所有账号已用尽）: {} {}",
                        api_type,
                        status,
                        body
                    );
                }

                last_error = Some(anyhow::anyhow!(
                    "{} API 请求失败: {} {}",
                    api_type,
                    status,
                    body
                ));
                continue;
            }

            // 429 Too Many Requests - 限流：驱逐 sticky cache + rotation bias 轮转 + 封禁当前端点桶
            if status.as_u16() == 429 {
                tracing::warn!(
                    "API 请求失败（上游限流，{}重试，尝试 {}/{}）: {} {}",
                    if small_pool {
                        "单账号延长间隔"
                    } else {
                        "切换账号"
                    },
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );
                self.token_manager.report_throttled(ctx.id);
                self.token_manager.report_throttled_for_rotation(ctx.id);
                // 端点级：封禁当前命中桶 30s（不动其它桶，不动账号）
                self.endpoint_registry
                    .throttle(ctx.id, endpoint.name, BUCKET_THROTTLE_DURATION);
                if let Some(cid) = continuation_id.as_deref() {
                    self.token_manager.report_sticky_throttled(cid, ctx.id);
                }
                if !throttled_in_request.contains(&ctx.id) {
                    throttled_in_request.push(ctx.id);
                }
                if let Some(ref store) = self.throttle_log_store {
                    store.record(
                        ctx.id,
                        "api",
                        status.as_u16(),
                        &body,
                        Some(endpoint.name.as_str()),
                    );
                }
                last_error = Some(anyhow::anyhow!(
                    "{} API 请求失败: {} {}",
                    api_type,
                    status,
                    body
                ));
                if attempt + 1 < max_retries {
                    let delay = Self::throttle_delay(attempt);
                    drop(_cred_permit);
                    sleep(delay).await;
                }
                continue;
            }

            // 408/5xx - 瞬态上游错误：重试但不禁用或切换账号
            // （避免 502 high load 等瞬态错误把所有账号锁死）
            if status.as_u16() == 408 || status.is_server_error() {
                tracing::warn!(
                    "API 请求失败（上游瞬态错误，尝试 {}/{}）: {} {}",
                    attempt + 1,
                    max_retries,
                    status,
                    body
                );
                last_error = Some(anyhow::anyhow!(
                    "{} API 请求失败: {} {}",
                    api_type,
                    status,
                    body
                ));
                if attempt + 1 < max_retries {
                    drop(_cred_permit);
                    sleep(Self::retry_delay(attempt)).await;
                }
                continue;
            }

            // 其他 4xx - 通常为请求/配置问题：直接返回，不计入账号失败
            if status.is_client_error() {
                anyhow::bail!("{} API 请求失败: {} {}", api_type, status, body);
            }

            // 兜底：当作可重试的瞬态错误处理（不切换账号）
            tracing::warn!(
                "API 请求失败（未知错误，尝试 {}/{}）: {} {}",
                attempt + 1,
                max_retries,
                status,
                body
            );
            last_error = Some(anyhow::anyhow!(
                "{} API 请求失败: {} {}",
                api_type,
                status,
                body
            ));
            if attempt + 1 < max_retries {
                drop(_cred_permit);
                sleep(Self::retry_delay(attempt)).await;
            }
        }

        // 所有重试都失败
        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!(
                "{} API 请求失败：已达到最大重试次数（{}次）",
                api_type,
                max_retries
            )
        }))
    }
}
