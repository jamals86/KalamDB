# System Tests (backend/tests/misc/system)

## Standard Steps
1. Arrange system-level configuration or users.
2. Execute the system operation described by the test name.
3. Assert system table state and audit metadata.

## Tests
- test_audit_log_for_user_management — [backend/tests/misc/system/test_audit_logging.rs](backend/tests/misc/system/test_audit_logging.rs#L49)
- test_audit_log_for_table_access_change — [backend/tests/misc/system/test_audit_logging.rs](backend/tests/misc/system/test_audit_logging.rs#L90)
- test_audit_log_password_masking — [backend/tests/misc/system/test_audit_logging.rs](backend/tests/misc/system/test_audit_logging.rs#L139)
- test_parameter_limits_from_config — [backend/tests/misc/system/test_config_access.rs](backend/tests/misc/system/test_config_access.rs#L13)
- test_config_accessible_from_app_context — [backend/tests/misc/system/test_config_access.rs](backend/tests/misc/system/test_config_access.rs#L51)
- test_live_queries_metadata — [backend/tests/misc/system/test_live_queries_metadata.rs](backend/tests/misc/system/test_live_queries_metadata.rs#L15)
- test_system_user_created_on_init — [backend/tests/misc/system/test_system_user_init.rs](backend/tests/misc/system/test_system_user_init.rs#L11)
- test_system_user_initialization_idempotent — [backend/tests/misc/system/test_system_user_init.rs](backend/tests/misc/system/test_system_user_init.rs#L57)
- test_system_user_localhost_no_password — [backend/tests/misc/system/test_system_users.rs](backend/tests/misc/system/test_system_users.rs#L74)
- test_system_user_remote_denied_by_default — [backend/tests/misc/system/test_system_users.rs](backend/tests/misc/system/test_system_users.rs#L128)
- test_system_user_remote_with_password — [backend/tests/misc/system/test_system_users.rs](backend/tests/misc/system/test_system_users.rs#L182)
- test_system_user_remote_no_password_denied — [backend/tests/misc/system/test_system_users.rs](backend/tests/misc/system/test_system_users.rs#L239)
- test_global_remote_access_flag — [backend/tests/misc/system/test_system_users.rs](backend/tests/misc/system/test_system_users.rs#L291)
