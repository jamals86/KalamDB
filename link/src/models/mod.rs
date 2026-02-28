//! Data models for kalam-link client library.
//!
//! Defines request and response structures for query execution and
//! WebSocket subscription messages.

pub mod ack_response;
pub mod batch_control;
pub mod batch_status;
pub mod change_event;
pub mod change_type_raw;
pub mod client_message;
pub mod connection_options;
pub mod consume_message;
pub mod consume_request;
pub mod consume_response;
pub mod error_detail;
pub mod field_flag;
pub mod health_check_response;
pub mod http_version;
pub mod kalam_data_type;
pub mod login_request;
pub mod login_response;
pub mod login_user_info;
pub mod query_request;
pub mod query_response;
pub mod query_result;
pub mod response_status;
pub mod schema_field;
pub mod server_message;
pub mod setup_models;
pub mod subscription_config;
pub mod subscription_info;
pub mod subscription_options;
pub mod subscription_request;
pub mod upload_progress;
pub mod username;
pub mod utils;
pub mod ws_auth_credentials;

#[cfg(test)]
mod tests;

pub use ack_response::AckResponse;
pub use batch_control::BatchControl;
pub use batch_status::BatchStatus;
pub use change_event::ChangeEvent;
pub use change_type_raw::ChangeTypeRaw;
pub use client_message::ClientMessage;
pub use connection_options::ConnectionOptions;
pub use consume_message::ConsumeMessage;
pub use consume_request::ConsumeRequest;
pub use consume_response::ConsumeResponse;
pub use error_detail::ErrorDetail;
pub use field_flag::{FieldFlag, FieldFlags};
pub use health_check_response::HealthCheckResponse;
pub use http_version::HttpVersion;
pub use kalam_data_type::KalamDataType;
pub use login_request::LoginRequest;
pub use login_response::LoginResponse;
pub use login_user_info::LoginUserInfo;
pub use query_request::QueryRequest;
pub use query_response::QueryResponse;
pub use query_result::QueryResult;
pub use response_status::ResponseStatus;
pub use schema_field::SchemaField;
pub use server_message::ServerMessage;
pub use setup_models::{
    ServerSetupRequest, ServerSetupResponse, SetupStatusResponse, SetupUserInfo,
};
pub use subscription_config::SubscriptionConfig;
pub use subscription_info::SubscriptionInfo;
pub use subscription_options::SubscriptionOptions;
pub use subscription_request::SubscriptionRequest;
pub use upload_progress::UploadProgress;
pub use username::Username;
pub use utils::parse_i64;
pub use ws_auth_credentials::WsAuthCredentials;
