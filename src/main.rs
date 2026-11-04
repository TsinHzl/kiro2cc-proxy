// Copyright (c) 2026 Harllan He. Licensed under MIT.

//! 程序入口：初始化日志/配置/凭据管理器，装配 axum 路由（Anthropic、OpenAI 兼容层、Admin/User API 与静态 UI）并启动 HTTP 服务。

mod admin;
mod admin_ui;
mod anthropic;
mod cache;
mod common;
mod http_client;
mod kiro;
mod log_capture;
mod logging;
mod model;
mod native;
mod openai;
pub mod token;
mod user;
mod user_ui;

use std::sync::Arc;

use clap::Parser;
use kiro::model::credentials::{CredentialsConfig, KiroCredentials};
use kiro::provider::KiroProvider;
use kiro::token_manager::MultiTokenManager;
use model::api_key::ApiKeyManager;
use model::arg::Args;
use model::config::Config;
use model::failure_log::FailureLogStore;
use model::throttle_log::ThrottleLogStore;
use model::usage::UsageTracker;

const LOG_CAPTURE_CAPACITY: usize = 100_000;

#[tokio::main]
async fn main() {
    // 解析命令行参数
    let args = Args::parse();

    // 加载配置（须先于 tracing 初始化：log_file 日志层需要按配置决定是否挂载；
    // 此时尚无日志设施，加载失败用 eprintln! 输出后退出）
    let config_path = args
        .config
        .unwrap_or_else(|| Config::default_config_path().to_string());
    let mut config = Config::load(&config_path).unwrap_or_else(|e| {
        eprintln!("加载配置失败: {}", e);
        std::process::exit(1);
    });

    // 环境变量覆盖配置（用于容器化部署）
    config.apply_env_overrides();

    // 初始化日志捕获器（在 tracing 初始化之前创建）
    let log_capture = std::sync::Arc::new(log_capture::LogCapture::new(LOG_CAPTURE_CAPACITY));

    // 初始化日志（registry 风格，同时输出到控制台、LogCapture 与可选的日志文件）
    {
        use tracing_subscriber::prelude::*;
        // 单一全局可重载过滤器：控制台 / 日志文件 / LogCapture 三处共用同一档位，
        // 运行时通过 logging::set_debug_enabled 热切换（reload 内部会重建 callsite
        // interest 缓存，切换即时生效）
        let (log_filter, log_filter_handle) = logging::new_reload_layer(config.debug_logs);
        logging::install(log_filter_handle);
        // log_file 配置时以 append 模式同步写文件；父目录不存在则自动创建；
        // 打开失败仅告警降级（只输出控制台），不影响启动
        let mut file_layer = None;
        if let Some(path) = config.log_file.as_ref() {
            let path = std::path::Path::new(path);
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                eprintln!(
                    "创建日志目录 {} 失败（{}），日志仅输出控制台",
                    parent.display(),
                    e
                );
            } else {
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    Ok(file) => {
                        file_layer = Some(
                            tracing_subscriber::fmt::layer().with_writer(std::sync::Arc::new(file)),
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "打开日志文件 {} 失败（{}），日志仅输出控制台",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }
        let registry = tracing_subscriber::registry()
            .with(log_filter)
            .with(tracing_subscriber::fmt::layer())
            .with(log_capture.as_layer());
        let log_file_enabled = file_layer.is_some();
        if let Some(layer) = file_layer {
            registry.with(layer).init();
        } else {
            registry.init();
        }
        // init 之后才能正常输出（subscriber 未设置前 tracing 宏会静默丢弃事件）
        if log_file_enabled {
            tracing::info!(
                "日志文件已启用: {}（append 模式）",
                config.log_file.as_ref().unwrap()
            );
        }
    }

    // 客户端 token 直通开关（true 时 usage 字段 1:1 上报，见 Issue #44）
    anthropic::set_client_token_passthrough(config.client_token_passthrough);
    anthropic::set_output_tokens_include_thinking(config.output_tokens_include_thinking);
    anthropic::set_models_official_aliases(config.models_official_aliases);
    anthropic::set_thinking_as_text(config.thinking_as_text);
    anthropic::set_show_session_credits(config.show_session_credits);
    if config.show_session_credits {
        tracing::info!(
            "会话 credits 汇总行已开启（showSessionCredits=true）：Claude Code 每次对话结束时在末尾展示 turn / session 消耗"
        );
    }
    anthropic::set_write_chunking_max_lines(config.write_chunking_max_lines);
    anthropic::set_kiro_body_dump_enabled(config.dump_kiro_body);
    if config.dump_kiro_body {
        tracing::warn!(
            "已开启 Kiro 请求体落盘取证（dumpKiroBody=true）：每个请求体将写入 app/config/kiro-body-dump/，定位问题后请关闭"
        );
    }
    if config.write_chunking_max_lines == 0 {
        tracing::info!(
            "分块写入约束已关闭（writeChunkingMaxLines=0）：大块工具参数可能触发上游约 240s 重置"
        );
    } else {
        tracing::info!(
            "分块写入约束行数上限: {}（writeChunkingMaxLines）",
            config.write_chunking_max_lines
        );
    }
    if config.thinking_as_text {
        tracing::info!(
            "思考文本化已开启（thinkingAsText=true）：Claude Code 的 thinking 将以引用文本逐行展示"
        );
    }
    if config.client_token_passthrough {
        tracing::info!(
            "已启用客户端 token 直通（clientTokenPassthrough），usage 字段将 1:1 上报真实值"
        );
    }
    if config.debug_logs {
        tracing::info!(
            "DEBUG 日志已开启（debugLogs=true）：控制台、日志文件与 Admin 日志页将输出 DEBUG 级别事件"
        );
    }

    // 加载凭证（支持单对象或数组格式）
    let credentials_path = args
        .credentials
        .unwrap_or_else(|| KiroCredentials::default_credentials_path().to_string());
    let credentials_config = CredentialsConfig::load(&credentials_path).unwrap_or_else(|e| {
        tracing::error!("加载凭证失败: {}", e);
        std::process::exit(1);
    });

    // 判断是否为多账号格式（用于刷新后回写）
    let is_multiple_format = credentials_config.is_multiple();

    // 转换为按优先级排序的账号列表
    let credentials_list = credentials_config.into_sorted_credentials();
    tracing::info!("已加载 {} 个账号配置", credentials_list.len());

    // 获取第一个账号用于日志显示
    let first_credentials = credentials_list.first().cloned().unwrap_or_default();
    tracing::debug!("主凭证: {:?}", first_credentials);

    // 构建代理配置
    let proxy_config = config.proxy_url.as_ref().map(|url| {
        let mut proxy = http_client::ProxyConfig::new(url);
        if let (Some(username), Some(password)) = (&config.proxy_username, &config.proxy_password) {
            proxy = proxy.with_auth(username, password);
        }
        proxy
    });

    if proxy_config.is_some() {
        tracing::info!("已配置 HTTP 代理: {}", config.proxy_url.as_ref().unwrap());
    }

    // 创建 MultiTokenManager 和 KiroProvider
    let token_manager = MultiTokenManager::new(
        config.clone(),
        credentials_list,
        proxy_config.clone(),
        Some(credentials_path.into()),
        is_multiple_format,
    )
    .unwrap_or_else(|e| {
        tracing::error!("创建 Token 管理器失败: {}", e);
        std::process::exit(1);
    });
    let token_manager = Arc::new(token_manager);

    // 账号保活：后台定期刷新长期闲置的账号（始终开启，间隔由 credentialKeepAliveHours 配置）
    kiro::token_manager::set_keep_alive_hours(config.credential_keep_alive_hours);
    tracing::info!(
        "账号保活已开启：距上次刷新超过 {} 小时的闲置账号将被定期刷新",
        kiro::token_manager::keep_alive_hours()
    );
    token_manager.spawn_keep_alive();

    // 创建 RPM 追踪器
    let rpm_tracker = Arc::new(model::rpm::RpmTracker::new());

    // 加载限流日志存储
    let throttle_data_dir = std::path::Path::new(&config_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let throttle_log_store = Arc::new(
        ThrottleLogStore::load(throttle_data_dir.join("throttle_log.json")).unwrap_or_else(|e| {
            tracing::warn!("加载限流日志失败（将使用空日志）: {}", e);
            ThrottleLogStore::empty(throttle_data_dir.join("throttle_log.json"))
        }),
    );

    let failure_log_store = Arc::new(
        FailureLogStore::load(throttle_data_dir.join("failure_log.json")).unwrap_or_else(|e| {
            tracing::warn!("加载失败日志失败（将使用空日志）: {}", e);
            FailureLogStore::empty(throttle_data_dir.join("failure_log.json"))
        }),
    );
    tracing::info!(
        "failure_log_store 已启用: {:?}",
        throttle_data_dir.join("failure_log.json")
    );

    // 共享同一个 provider 实例：Anthropic 兼容路由与 /native 原生直通路由
    // 必须共用账号池、并发信号量与限流状态。
    let kiro_provider = Arc::new(
        KiroProvider::with_proxy(token_manager.clone(), proxy_config.clone())
            .with_rpm_tracker(rpm_tracker.clone())
            .with_throttle_log_store(throttle_log_store.clone())
            .with_failure_log_store(failure_log_store.clone()),
    );

    // 初始化 count_tokens 配置
    token::init_config(token::CountTokensConfig {
        api_url: config.count_tokens_api_url.clone(),
        api_key: config.count_tokens_api_key.clone(),
        auth_type: config.count_tokens_auth_type.clone(),
        proxy: proxy_config,
        tls_backend: config.tls_backend,
    });

    // 初始化 API Key 管理器和用量追踪器（Admin 启用时才加载）
    let admin_key_valid = config
        .admin_psw
        .as_ref()
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);

    let (api_key_manager, usage_tracker) = if admin_key_valid {
        let data_dir = std::path::Path::new(&config_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));

        let manager = ApiKeyManager::load(data_dir.join("api_keys.json")).unwrap_or_else(|e| {
            tracing::error!("加载 API Key 数据失败: {}", e);
            std::process::exit(1);
        });
        let manager = Arc::new(manager);

        let tracker = UsageTracker::load(data_dir.join("api_key_usage.json")).unwrap_or_else(|e| {
            tracing::error!("加载用量数据失败: {}", e);
            std::process::exit(1);
        });
        let tracker = Arc::new(tracker);

        // 用历史用量记录中出现过的最大 api_key_id 补齐 ID 计数器种子。
        // 计数器文件首次不存在时（所有存量部署），仅凭当前 key 列表推算会漏掉已删除
        // key 曾用过的高位 id，新 key 复用后会继承其用量与累计消费额。
        // 此刻尚未开始服务请求，无并发风险。
        manager.seed_id_counter_at_least(tracker.max_api_key_id());

        tracing::info!("API Key 多用户管理已启用");
        (Some(manager), Some(tracker))
    } else {
        (None, None)
    };

    // 启动 prompt cache 指纹追踪器（生产模式自动启动 30s 周期 evict 后台任务）
    let fingerprint_tracker = cache::fingerprint::FingerprintTracker::new(config.cache_simulation);
    tracing::info!(
        "FingerprintTracker 启动 (enabled={}, ttl_5m={}s, max_breakpoints={})",
        config.cache_simulation.fingerprint_enabled,
        config.cache_simulation.fingerprint_ttl_5m,
        config
            .cache_simulation
            .fingerprint_max_breakpoints_per_account
    );

    // Suggestion Mode 放行开关：anthropic AppState 与 AdminState 共享同一 Arc 实例，
    // Admin API（PUT /api/admin/config/suggestion-mode）修改后即时生效
    let suggestion_mode = Arc::new(std::sync::atomic::AtomicBool::new(
        config.forward_suggestion_mode,
    ));
    let mut anthropic_app_state = anthropic::middleware::AppState::new()
        .with_rpm_tracker(rpm_tracker.clone())
        .with_forward_suggestion_mode(config.forward_suggestion_mode)
        .with_forward_suggestion_mode_arc(suggestion_mode.clone())
        .with_fingerprint_tracker(fingerprint_tracker.clone());
    if config.prompt_cache_accounting {
        let snapshot = throttle_data_dir.join("prompt_cache.json");
        let cache = Arc::new(cache::breakpoint::BreakpointCache::new(
            config.prompt_cache_min_tokens,
            Some(snapshot.clone()),
        ));
        cache.start_maintenance();
        tracing::info!(
            "Prompt cache 断点记账已启用 (min_tokens={}, 已恢复 {} 条, 快照 {:?})",
            if config.prompt_cache_min_tokens > 0 {
                config.prompt_cache_min_tokens.to_string()
            } else {
                "按模型".to_string()
            },
            cache.len(),
            snapshot
        );
        anthropic_app_state = anthropic_app_state.with_breakpoint_cache(cache);
    }
    if config.forward_suggestion_mode {
        tracing::info!(
            "Suggestion Mode 输入建议请求已放行（forward_suggestion_mode=true），将正常转发上游"
        );
    }
    if let Some(ref manager) = api_key_manager {
        anthropic_app_state = anthropic_app_state.with_api_key_manager(manager.clone());
    }
    if let Some(ref tracker) = usage_tracker {
        anthropic_app_state = anthropic_app_state.with_usage_tracker(tracker.clone());
    }

    let anthropic_app = anthropic::create_router_with_provider_and_state(
        anthropic_app_state,
        Some(kiro_provider.clone()),
        first_credentials.profile_arn.clone(),
    );

    // 构建 Admin API 路由（如果配置了非空的 admin_psw）
    let app = if let Some(admin_key) = &config.admin_psw {
        if admin_key.trim().is_empty() {
            tracing::warn!("admin_psw 配置为空，Admin API 未启用");
            anthropic_app
        } else {
            let geo_resolver = Arc::new(
                model::geo::GeoResolver::new(throttle_data_dir).unwrap_or_else(|e| {
                    tracing::error!("初始化 IP 归属地解析器失败: {}", e);
                    std::process::exit(1);
                }),
            );

            let admin_service = admin::AdminService::new(token_manager.clone())
                .with_throttle_log_store(throttle_log_store.clone());
            let admin_psw_shared = Arc::new(parking_lot::RwLock::new(admin_key.clone()));
            let mut admin_state = admin::AdminState::new(admin_psw_shared, admin_service)
                .with_rpm_tracker(rpm_tracker.clone())
                .with_config_path(std::path::PathBuf::from(&config_path))
                .with_geo_resolver(geo_resolver.clone());
            if let Some(ref manager) = api_key_manager {
                admin_state = admin_state.with_api_key_manager(manager.clone());
            }
            if let Some(ref tracker) = usage_tracker {
                admin_state = admin_state.with_usage_tracker(tracker.clone());
            }
            // 对话页会话存储：失败只影响历史会话功能，不阻止代理服务启动
            match model::chat_session::ChatSessionStore::load(
                throttle_data_dir.join("chat_sessions"),
            ) {
                Ok(store) => admin_state = admin_state.with_chat_session_store(Arc::new(store)),
                Err(e) => tracing::error!("加载对话会话失败，历史会话功能不可用: {}", e),
            }
            admin_state = admin_state.with_throttle_log_store(throttle_log_store.clone());
            admin_state = admin_state.with_failure_log_store(failure_log_store.clone());
            admin_state = admin_state.with_log_capture(log_capture.clone());
            admin_state = admin_state.with_suggestion_mode(suggestion_mode.clone());
            let admin_app = admin::create_admin_router(admin_state);

            // 创建 Admin UI 路由
            let admin_ui_app = admin_ui::create_admin_ui_router();

            // 创建 User API 路由
            let user_state = user::UserState {
                api_key_manager: api_key_manager.clone().unwrap(),
                usage_tracker: usage_tracker.clone().unwrap(),
                geo_resolver: geo_resolver.clone(),
            };
            let user_app = user::create_user_router(user_state);

            // 创建 User UI 路由
            let user_ui_app = user_ui::create_user_ui_router();

            tracing::info!("Admin API 已启用");
            tracing::info!("Admin UI 已启用: /admin");
            tracing::info!("User API 已启用: /api/user");
            tracing::info!("User UI 已启用: /user");
            anthropic_app
                .nest("/api/admin", admin_app)
                .nest("/admin", admin_ui_app)
                .nest("/api/user", user_app)
                .nest("/user", user_ui_app)
        }
    } else {
        anthropic_app
    };

    // 原生 Kiro 直通入口：与 Anthropic/OpenAI/Admin 共用同一端口与 provider，
    // 但路由/状态独立（不改动既有协议行为）。sub2api 原生转发层调用 POST /native。
    let app = app.merge(native::create_router(kiro_provider.clone()));

    // 启动服务器
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("启动 Anthropic API 端点: {}", addr);
    tracing::info!("可用 API:");
    tracing::info!("  GET  /v1/models");
    tracing::info!("  POST /v1/messages");
    tracing::info!("  POST /v1/messages/count_tokens");
    tracing::info!("  POST /v1/chat/completions  (OpenAI 兼容)");
    tracing::info!("  POST /v1/responses         (OpenAI 兼容 / Codex CLI)");
    tracing::info!("  POST /native               (Kiro 原生协议直通，x-amz-target 分发)");
    if admin_key_valid {
        tracing::info!("Admin API:");
        tracing::info!("  GET  /api/admin/credentials");
        tracing::info!("  POST /api/admin/credentials/:index/disabled");
        tracing::info!("  POST /api/admin/credentials/:index/priority");
        tracing::info!("  POST /api/admin/credentials/:index/reset");
        tracing::info!("  GET  /api/admin/credentials/:index/balance");
        tracing::info!("Admin UI:");
        tracing::info!("  GET  /admin");
        tracing::info!("User API:");
        tracing::info!("  POST /api/user/login");
        tracing::info!("  GET  /api/user/usage");
        tracing::info!("User UI:");
        tracing::info!("  GET  /user");
    }

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        tracing::error!(
            "监听地址 {} 失败: {}。可能是端口被其他进程占用，或前一个进程尚未完全释放该端口，请稍后重试或更换端口。",
            addr,
            e
        );
        std::process::exit(1);
    });
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}
