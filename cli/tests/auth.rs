// Aggregator for authentication tests to ensure Cargo picks them up
//
// Run these tests with:
//   cargo test --test auth
//
// Run individual test files:
//   cargo test --test auth test_auth

mod common;

#[path = "auth/test_auth.rs"]
mod test_auth;
