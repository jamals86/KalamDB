// KalamDB API Library
//
// This crate provides the REST API layer for KalamDB,
// including HTTP handlers, routes, and request/response models.

pub mod actors;
pub mod auth;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod rate_limiter;
pub mod repositories;
pub mod routes;
