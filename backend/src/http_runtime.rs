//! Lightweight HTTP runtime state shared by Actix workers.

use std::sync::Arc;

use actix_web::web;
use anyhow::Result;
use kalamdb_api::{limiter::RateLimiter, ui::UiRuntimeConfig};
use kalamdb_auth::UserRepository;
use kalamdb_configs::{AuthSettings, CorsSettings, ServerConfig};
use kalamdb_core::{
    app_context::AppContext,
    sql::{datafusion_session::DataFusionSessionFactory, executor::SqlExecutor},
};
use kalamdb_live::{ConnectionsManager, LiveQueryManager};

use crate::{
    lifecycle::ApplicationComponents, middleware::ConnectionProtection,
    startup::configure_auth_runtime,
};

#[derive(Clone, Copy)]
pub enum AuthRuntimeMode {
    Configure,
    AlreadyConfigured,
}

#[derive(Clone)]
pub struct HttpRuntimeState {
    pub app_context: web::Data<Arc<AppContext>>,
    pub session_factory: web::Data<Arc<DataFusionSessionFactory>>,
    pub sql_executor: web::Data<Arc<SqlExecutor>>,
    pub rate_limiter: web::Data<Arc<RateLimiter>>,
    pub live_query_manager: web::Data<Arc<LiveQueryManager>>,
    pub user_repo: web::Data<Arc<dyn UserRepository>>,
    pub connection_registry: web::Data<Arc<ConnectionsManager>>,
    pub auth_settings: web::Data<AuthSettings>,
    pub connection_protection: ConnectionProtection,
    pub cors_settings: Arc<CorsSettings>,
    pub ui_path: Option<String>,
    pub ui_runtime_config: UiRuntimeConfig,
    ui_status: &'static str,
}

impl HttpRuntimeState {
    pub fn new(
        config: &ServerConfig,
        components: &ApplicationComponents,
        app_context: Arc<AppContext>,
        auth_runtime_mode: AuthRuntimeMode,
    ) -> Result<Self> {
        if matches!(auth_runtime_mode, AuthRuntimeMode::Configure) {
            configure_auth_runtime(config)?;
        }

        let ui_path = config.server.ui_path.clone();
        let ui_status = if kalamdb_api::routes::is_embedded_ui_available() {
            "embedded in binary"
        } else if ui_path.is_some() {
            "filesystem"
        } else {
            "disabled"
        };

        Ok(Self {
            app_context: web::Data::new(app_context),
            session_factory: web::Data::new(components.session_factory.clone()),
            sql_executor: web::Data::new(components.sql_executor.clone()),
            rate_limiter: web::Data::new(components.rate_limiter.clone()),
            live_query_manager: web::Data::new(components.live_query_manager.clone()),
            user_repo: web::Data::new(components.user_repo.clone()),
            connection_registry: web::Data::new(components.connection_registry.clone()),
            auth_settings: web::Data::new(config.auth.clone()),
            connection_protection: ConnectionProtection::from_server_config(config),
            cors_settings: Arc::new(config.security.cors.clone()),
            ui_path,
            ui_runtime_config: UiRuntimeConfig::new(config.server.configured_public_origin()),
            ui_status,
        })
    }

    pub fn ui_status(&self) -> &'static str {
        self.ui_status
    }
}
