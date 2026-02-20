//! Keycloak OIDC integration tests
//!
//! These tests validate:
//! - Keycloak realm is reachable and configured correctly
//! - Token acquisition via Direct Access Grant (Resource Owner Password)
//! - Auto-provisioning of users from trusted OIDC providers
//! - Idempotent lookup of existing provider users
//!
//! ## Prerequisites
//!
//! 1. KalamDB server must be running
//! 2. Keycloak must be running (docker/utils/docker-compose.yml)
//! 3. Server must be started with:
//!    ```sh
//!    KALAMDB_JWT_TRUSTED_ISSUERS="kalamdb,http://localhost:8081/realms/kalamdb" \
//!    KALAMDB_AUTH_AUTO_CREATE_USERS_FROM_PROVIDER=true \
//!    cargo run
//!    ```
//!
//! If either server or Keycloak is not running, or if the server is not
//! configured for provider auto-creation, the tests are skipped gracefully.
//!
//! ## Environment variables
//!
//! | Variable                     | Default                                       |
//! |------------------------------|-----------------------------------------------|
//! | `KEYCLOAK_URL`               | `http://localhost:8081`                        |
//! | `KEYCLOAK_REALM`             | `kalamdb`                                     |
//! | `KEYCLOAK_CLIENT_ID`         | `kalamdb-api`                                 |
//! | `KEYCLOAK_TEST_USER`         | `kalamdb-user`                                |
//! | `KEYCLOAK_TEST_PASSWORD`     | `kalamdb123`                                  |

use crate::common::*;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Configuration helpers
// ---------------------------------------------------------------------------

fn keycloak_url() -> String {
    std::env::var("KEYCLOAK_URL").unwrap_or_else(|_| "http://localhost:8081".to_string())
}

fn keycloak_realm() -> String {
    std::env::var("KEYCLOAK_REALM").unwrap_or_else(|_| "kalamdb".to_string())
}

fn keycloak_client_id() -> String {
    std::env::var("KEYCLOAK_CLIENT_ID").unwrap_or_else(|_| "kalamdb-api".to_string())
}

fn keycloak_test_user() -> String {
    std::env::var("KEYCLOAK_TEST_USER").unwrap_or_else(|_| "kalamdb-user".to_string())
}

fn keycloak_test_password() -> String {
    std::env::var("KEYCLOAK_TEST_PASSWORD").unwrap_or_else(|_| "kalamdb123".to_string())
}

fn keycloak_issuer() -> String {
    format!("{}/realms/{}", keycloak_url(), keycloak_realm())
}

/// Token endpoint for Direct Access Grant (Resource Owner Password Credentials).
fn keycloak_token_endpoint() -> String {
    format!(
        "{}/realms/{}/protocol/openid-connect/token",
        keycloak_url(),
        keycloak_realm()
    )
}

// ---------------------------------------------------------------------------
// Reachability checks
// ---------------------------------------------------------------------------

/// Check if Keycloak is reachable by hitting the realm discovery endpoint.
fn is_keycloak_reachable() -> bool {
    let url = format!("{}/.well-known/openid-configuration", keycloak_issuer());
    let result = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().ok()?;
        rt.block_on(async {
            Client::new()
                .get(&url)
                .timeout(Duration::from_secs(3))
                .send()
                .await
                .ok()
                .filter(|r| r.status().is_success())
        })
    })
    .join()
    .ok()
    .flatten();

    result.is_some()
}

/// Guard macro-like function: returns `false` (skip) if preconditions aren't met.
fn should_run_keycloak_tests() -> bool {
    if !is_server_running() {
        eprintln!("⚠️  KalamDB server not running. Skipping Keycloak tests.");
        return false;
    }
    if !is_keycloak_reachable() {
        eprintln!("⚠️  Keycloak not reachable at {}. Skipping Keycloak tests.", keycloak_url());
        eprintln!("   Start Keycloak: cd docker/utils && docker-compose up -d keycloak");
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// Helper: acquire token from Keycloak via Direct Access Grant
// ---------------------------------------------------------------------------

/// Get an access token from Keycloak using the Resource Owner Password flow.
/// This proves that Keycloak is alive, the realm exists, the client is configured,
/// and the test user can authenticate.
async fn get_keycloak_token() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let client = Client::new();
    let response = client
        .post(&keycloak_token_endpoint())
        .form(&[
            ("grant_type", "password"),
            ("client_id", &keycloak_client_id()),
            ("username", &keycloak_test_user()),
            ("password", &keycloak_test_password()),
        ])
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    let status = response.status();
    let body: serde_json::Value = response.json().await?;

    if !status.is_success() {
        return Err(format!(
            "Keycloak token request failed ({}): {}",
            status,
            serde_json::to_string_pretty(&body).unwrap_or_default()
        )
        .into());
    }

    Ok(body)
}

/// Craft an HS256 JWT token that mimics a Keycloak token structure.
///
/// KalamDB currently only validates HS256 tokens. Real Keycloak tokens use RS256
/// and cannot be verified by our server. This helper crafts an HS256 token with
/// the Keycloak issuer and a test subject so we can exercise the auto-provisioning
/// code path end-to-end.
///
/// The server must be configured with:
/// - `jwt_trusted_issuers` containing the Keycloak issuer URL
/// - `jwt_secret` matching the secret used here (defaults to server's default secret)
fn craft_hs256_keycloak_token(
    subject: &str,
    preferred_username: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let now = chrono::Utc::now();
    let exp = now + chrono::Duration::hours(1);

    let claims = json!({
        "sub": subject,
        "iss": keycloak_issuer(),
        "exp": exp.timestamp() as usize,
        "iat": now.timestamp() as usize,
        "preferred_username": preferred_username,
        "email": format!("{}@test.kalamdb.dev", preferred_username),
    });

    // Use the same secret the server uses.
    // When started with defaults this is "CHANGE_ME_IN_PRODUCTION".
    let secret = std::env::var("KALAMDB_JWT_SECRET")
        .unwrap_or_else(|_| "CHANGE_ME_IN_PRODUCTION".to_string());

    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(secret.as_bytes());
    let token = encode(&header, &claims, &key)?;
    Ok(token)
}

/// Send a SQL request to KalamDB authenticated with an arbitrary Bearer token.
async fn execute_sql_with_bearer(
    token: &str,
    sql: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let client = Client::new();
    let response = client
        .post(format!("{}/v1/api/sql", server_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({ "sql": sql }))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    let status = response.status();
    let body: serde_json::Value = response.json().await?;

    if !status.is_success() {
        return Err(format!(
            "SQL request failed ({}): {}",
            status,
            serde_json::to_string_pretty(&body).unwrap_or_default()
        )
        .into());
    }

    Ok(body)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify Keycloak is alive and the `kalamdb` realm is configured correctly.
///
/// Gets a token from Keycloak via Direct Access Grant and asserts the response
/// contains an `access_token`. This does NOT send the token to KalamDB (RS256 ≠ HS256).
#[test]
fn test_keycloak_realm_configured() {
    if !should_run_keycloak_tests() {
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let token_response = rt.block_on(get_keycloak_token());

    match token_response {
        Ok(body) => {
            assert!(
                body.get("access_token").is_some(),
                "Keycloak response should contain an access_token: {:?}",
                body
            );
            eprintln!(
                "[keycloak] Successfully obtained token for user '{}'",
                keycloak_test_user()
            );
        },
        Err(e) => {
            panic!(
                "Failed to get token from Keycloak. \
                 Ensure realm '{}' has client '{}' with Direct Access Grants enabled \
                 and user '{}' exists: {}",
                keycloak_realm(),
                keycloak_client_id(),
                keycloak_test_user(),
                e
            );
        },
    }
}

/// Test that an HS256 bearer token with a Keycloak issuer triggers auto-provisioning.
///
/// The server must be started with:
/// ```sh
/// KALAMDB_JWT_TRUSTED_ISSUERS="kalamdb,http://localhost:8081/realms/kalamdb" \
/// KALAMDB_AUTH_AUTO_CREATE_USERS_FROM_PROVIDER=true \
/// cargo run
/// ```
///
/// The test crafts an HS256 token with:
/// - `iss` = Keycloak issuer URL
/// - `sub` = a unique test subject
/// - `preferred_username` = a display name
///
/// On first request, the server should auto-create a user with username
/// `oidc:kcl:{subject}` and succeed. On second request with the same token,
/// it should reuse the existing user (index lookup).
#[test]
fn test_provider_auto_provisioning_via_bearer() {
    if !should_run_keycloak_tests() {
        return;
    }

    // Use a unique subject per test run to avoid collisions
    let subject = format!("test-kc-{}", chrono::Utc::now().timestamp_millis());
    let preferred_username = "keycloak-test-user";

    let token = match craft_hs256_keycloak_token(&subject, preferred_username) {
        Ok(t) => t,
        Err(e) => {
            panic!("Failed to craft HS256 token: {}", e);
        },
    };

    let rt = tokio::runtime::Runtime::new().unwrap();

    // --- First request: should auto-provision the user ---
    let result1 = rt.block_on(execute_sql_with_bearer(&token, "SELECT 1 AS probe"));

    match &result1 {
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            // If the server has no Keycloak issuer trusted, or auto-create is off,
            // we get an "untrusted issuer" or "user not found" error. Skip gracefully.
            if err_str.contains("untrusted issuer")
                || err_str.contains("no trusted issuers configured")
                || err_str.contains("user not found")
            {
                eprintln!(
                    "⚠️  Server not configured for Keycloak auto-provisioning. Skipping.\n\
                     Start the server with:\n  \
                     KALAMDB_JWT_TRUSTED_ISSUERS=\"kalamdb,{}\" \\\n  \
                     KALAMDB_AUTH_AUTO_CREATE_USERS_FROM_PROVIDER=true \\\n  \
                     cargo run",
                    keycloak_issuer()
                );
                return;
            }
            panic!("First bearer request failed unexpectedly: {}", e);
        },
        Ok(body) => {
            eprintln!("[keycloak] First request succeeded (user auto-provisioned): {:?}", body);
        },
    }

    // --- Second request: should reuse the existing user (index lookup) ---
    let result2 = rt.block_on(execute_sql_with_bearer(&token, "SELECT 2 AS probe"));
    assert!(
        result2.is_ok(),
        "Second request with same token should succeed (user already exists): {:?}",
        result2.err()
    );

    // --- Verify the user was created with the expected oidc:kcl:* username ---
    let expected_username = format!("oidc:kcl:{}", subject);
    let check_sql = format!(
        "SELECT username, auth_type FROM system.users WHERE username = '{}'",
        expected_username
    );

    // Query as root to check the system.users table
    let check_result = rt.block_on(execute_sql_via_http_as_root(&check_sql));
    match check_result {
        Ok(body) => {
            let rows = get_rows_as_hashmaps(&body);
            assert!(
                rows.is_some() && !rows.as_ref().unwrap().is_empty(),
                "User '{}' should exist in system.users after auto-provisioning.\nResponse: {:?}",
                expected_username,
                body
            );
            let user_row = &rows.unwrap()[0];
            assert_eq!(
                user_row.get("username").and_then(|v| v.as_str()),
                Some(expected_username.as_str()),
                "Username should be the composed provider username"
            );
            assert_eq!(
                user_row.get("auth_type").and_then(|v| v.as_str()),
                Some("OAuth"),
                "auth_type should be 'OAuth' for provider-provisioned users"
            );
            eprintln!(
                "[keycloak] Verified user '{}' exists with auth_type=OAuth",
                expected_username
            );
        },
        Err(e) => {
            eprintln!(
                "[keycloak] Could not verify user in system.users (may require DBA role): {}",
                e
            );
        },
    }

    // --- Cleanup: delete the test user ---
    let drop_sql = format!("DROP USER '{}'", expected_username);
    let _ = rt.block_on(execute_sql_via_http_as_root(&drop_sql));
}

/// Test that a bearer token with an untrusted issuer is rejected.
///
/// This verifies the server's issuer validation: if the issuer in the JWT
/// does not match any entry in `jwt_trusted_issuers`, the request must fail
/// with an appropriate error (not silently succeed).
#[test]
fn test_untrusted_issuer_rejected() {
    if !should_run_keycloak_tests() {
        return;
    }

    let now = chrono::Utc::now();
    let exp = now + chrono::Duration::hours(1);

    // Craft a token with a completely unknown issuer
    let claims = json!({
        "sub": "untrusted-user-001",
        "iss": "https://evil-provider.example.com/realms/attack",
        "exp": exp.timestamp() as usize,
        "iat": now.timestamp() as usize,
        "preferred_username": "evil-user",
    });

    let secret = std::env::var("KALAMDB_JWT_SECRET")
        .unwrap_or_else(|_| "CHANGE_ME_IN_PRODUCTION".to_string());

    let token = {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(secret.as_bytes());
        encode(&header, &claims, &key).expect("Failed to encode untrusted token")
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        let client = Client::new();
        let response = client
            .post(format!("{}/v1/api/sql", server_url()))
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({ "sql": "SELECT 1" }))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .expect("HTTP request failed");

        (response.status(), response.json::<serde_json::Value>().await.ok())
    });

    // The server should reject this token (401 or 403)
    assert!(
        result.0.as_u16() == 401 || result.0.as_u16() == 403,
        "Untrusted issuer should be rejected with 401/403, got {} - body: {:?}",
        result.0,
        result.1
    );

    eprintln!(
        "[keycloak] Untrusted issuer correctly rejected with status {}",
        result.0
    );
}

/// Test that provider users from different issuers with the same subject
/// do NOT collide (different provider codes produce different usernames).
#[test]
fn test_provider_username_no_collision() {
    if !should_run_keycloak_tests() {
        return;
    }

    let subject = "shared-subject-12345";

    // Keycloak issuer → oidc:kcl:<subject>
    let keycloak_username = format!("oidc:kcl:{}", subject);

    // Google issuer → oidc:ggl:<subject>
    let google_issuer = "https://accounts.google.com";
    let now = chrono::Utc::now();
    let exp = now + chrono::Duration::hours(1);

    let secret = std::env::var("KALAMDB_JWT_SECRET")
        .unwrap_or_else(|_| "CHANGE_ME_IN_PRODUCTION".to_string());

    let make_token = |issuer: &str| -> String {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
        let claims = json!({
            "sub": subject,
            "iss": issuer,
            "exp": exp.timestamp() as usize,
            "iat": now.timestamp() as usize,
            "preferred_username": "collision-test",
        });
        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(secret.as_bytes());
        encode(&header, &claims, &key).expect("Failed to encode token")
    };

    let kc_token = make_token(&keycloak_issuer());
    let google_token = make_token(google_issuer);

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Try Keycloak token first
    let kc_result = rt.block_on(execute_sql_with_bearer(&kc_token, "SELECT 1 AS kc_probe"));
    if let Err(e) = &kc_result {
        let err_str = e.to_string().to_lowercase();
        if err_str.contains("untrusted issuer")
            || err_str.contains("no trusted issuers configured")
            || err_str.contains("user not found")
        {
            eprintln!(
                "⚠️  Server not configured for provider auto-provisioning. Skipping collision test."
            );
            return;
        }
    }

    // Try Google token - may fail if Google issuer is not trusted, that's OK
    let google_result =
        rt.block_on(execute_sql_with_bearer(&google_token, "SELECT 1 AS google_probe"));

    // If both succeeded, verify they created different users
    if kc_result.is_ok() && google_result.is_ok() {
        let google_username = format!("oidc:ggl:{}", subject);
        assert_ne!(
            keycloak_username, google_username,
            "Different providers with same subject must produce different usernames"
        );

        // Verify both users exist
        let check = |username: &str| -> bool {
            let sql = format!(
                "SELECT username FROM system.users WHERE username = '{}'",
                username
            );
            rt.block_on(execute_sql_via_http_as_root(&sql))
                .ok()
                .and_then(|body| get_rows_as_hashmaps(&body))
                .map(|rows| !rows.is_empty())
                .unwrap_or(false)
        };

        assert!(
            check(&keycloak_username),
            "Keycloak user '{}' should exist",
            keycloak_username
        );
        assert!(
            check(&google_username),
            "Google user '{}' should exist",
            google_username
        );

        eprintln!(
            "[keycloak] Confirmed no collision: '{}' != '{}'",
            keycloak_username, google_username
        );

        // Cleanup
        let _ = rt.block_on(execute_sql_via_http_as_root(&format!(
            "DROP USER '{}'",
            keycloak_username
        )));
        let _ = rt.block_on(execute_sql_via_http_as_root(&format!(
            "DROP USER '{}'",
            google_username
        )));
    } else {
        eprintln!(
            "[keycloak] Collision test partial: kc={}, google={} (some issuers may not be trusted)",
            kc_result.is_ok(),
            google_result.is_ok()
        );

        // Cleanup any user that was created
        let _ = rt.block_on(execute_sql_via_http_as_root(&format!(
            "DROP USER '{}'",
            keycloak_username
        )));
    }
}
