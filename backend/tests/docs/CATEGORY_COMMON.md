# Common Test Helpers (backend/tests/common)

## Standard Steps
1. Arrange helper inputs for the utility under test.
2. Execute helper behavior described by the test name.
3. Assert helper output matches expected format.

## Tests
- test_create_basic_auth_header — [backend/tests/common/testserver/auth_helper.rs](backend/tests/common/testserver/auth_helper.rs#L271)
- test_authenticate_basic — [backend/tests/common/testserver/auth_helper.rs](backend/tests/common/testserver/auth_helper.rs#L282)
- test_create_namespace — [backend/tests/common/testserver/fixtures.rs](backend/tests/common/testserver/fixtures.rs#L552)
- test_create_messages_table — [backend/tests/common/testserver/fixtures.rs](backend/tests/common/testserver/fixtures.rs#L560)
- test_insert_sample_messages — [backend/tests/common/testserver/fixtures.rs](backend/tests/common/testserver/fixtures.rs#L578)
- test_generate_user_data — [backend/tests/common/testserver/fixtures.rs](backend/tests/common/testserver/fixtures.rs#L601)
- test_setup_complete_environment — [backend/tests/common/testserver/fixtures.rs](backend/tests/common/testserver/fixtures.rs#L610)
