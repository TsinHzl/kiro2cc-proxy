//! User API 路由配置

use axum::{
    Router, middleware,
    routing::{get, post},
};

use super::{
    handlers::{get_usage, login},
    middleware::{UserState, user_auth_middleware},
};

/// 创建 User API 路由
pub fn create_user_router(state: UserState) -> Router {
    // login 不需要鉴权
    let public = Router::new()
        .route("/login", post(login))
        .with_state(state.clone());

    // usage 需要鉴权
    let protected = Router::new()
        .route("/usage", get(get_usage))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            user_auth_middleware,
        ))
        .with_state(state);

    public.merge(protected)
}
