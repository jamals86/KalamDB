# Feature Specification: System Improvements and Performance Optimization

**Feature Branch**: `004-system-improvements-and`  
**Created**: October 21, 2025  
**Status**: Draft  
**Input**: User description: "System Improvements and Query Optimization - Parametrized queries, automatic flushing, caching, and architectural refactoring"

## User Scenarios & Testing *(mandatory)*

### User Story 0 - Kalam CLI: Interactive Command-Line Client (Priority: P0)

Database developers, administrators, and users need an interactive command-line client similar to `mysql` or `psql` for querying, managing, and subscribing to KalamDB data streams. The CLI should provide a familiar SQL shell experience with modern features like live query subscriptions, real-time data streaming, and SQL keyword auto-completion.

**Why this priority**: A command-line interface is essential for development, debugging, testing, and administration. It's the primary tool developers use to interact with databases during development and troubleshooting. Without a CLI, the only way to interact with KalamDB is through HTTP API calls or WebSocket connections, which is cumbersome for interactive work.

**Independent Test**: Can be fully tested by launching `kalam-cli` with connection parameters, executing SQL queries (SELECT, INSERT, CREATE TABLE), establishing WebSocket subscriptions with `SUBSCRIBE TO`, verifying all responses are displayed correctly in tabular or JSON format, and testing auto-completion of SQL keywords.

**Acceptance Scenarios**:

1. **Given** a user has KalamDB server running, **When** they execute `kalam-cli -u jamal -h http://localhost:8080 --token <jwt>`, **Then** the CLI connects successfully and displays a welcome prompt with user and server information
2. **Given** the CLI is connected, **When** a user types a SQL query like `SELECT * FROM messages LIMIT 10;`, **Then** the query executes and results are displayed in formatted table output
3. **Given** the CLI supports multiple output formats, **When** a user launches with `--json` flag or `--csv` flag, **Then** query results are formatted in the specified format instead of tables
4. **Given** a user wants to see available tables, **When** they execute `SHOW TABLES;`, **Then** all tables accessible to the user are listed in tabular format
5. **Given** a user wants to understand a table structure, **When** they execute `DESCRIBE messages;`, **Then** the table schema is displayed with columns, types, and nullable information
6. **Given** a user needs real-time updates, **When** they execute `SUBSCRIBE TO messages WHERE user_id = 'jamal';`, **Then** a WebSocket connection is established and live updates stream to the console
7. **Given** a live subscription is active, **When** data changes occur in the subscribed table, **Then** updates are displayed with timestamps and change indicators (INSERT/UPDATE/DELETE)
8. **Given** a live subscription is streaming, **When** the user presses Ctrl+C, **Then** the subscription stops and the CLI returns to the normal prompt
9. **Given** the CLI needs configuration, **When** a user runs the CLI for the first time, **Then** it creates a default config file at `~/.kalam/config.toml` with connection defaults
10. **Given** a user has a config file, **When** they launch `kalam-cli` without parameters, **Then** connection details are loaded from the config file
11. **Given** the CLI is running, **When** a user types `\quit` or `\q`, **Then** the CLI exits gracefully and closes all connections
12. **Given** the user needs help, **When** they type `\help`, **Then** all available commands and their descriptions are displayed
13. **Given** authentication is required, **When** the user provides `--token <jwt>` or `--apikey <key>`, **Then** the CLI authenticates using the provided credential
14. **Given** the CLI supports batch execution, **When** a user provides a SQL file with `kalam-cli --file queries.sql`, **Then** all queries in the file execute sequentially and output is displayed
15. **Given** a user is typing a SQL command, **When** they press TAB after typing "SEL", **Then** the CLI auto-completes to "SELECT"
16. **Given** a user is typing a SQL command, **When** they press TAB after a partial keyword, **Then** the CLI shows a list of matching SQL keywords (SELECT, INSERT, CREATE, etc.)

**Integration Tests** (backend/tests/integration/test_kalam_cli.rs):

1. **test_cli_connection_and_prompt**: Launch CLI with connection parameters, verify welcome message displays, verify prompt shows "kalam>" 
2. **test_cli_basic_query_execution**: Connect CLI, execute `SELECT 1 as test;`, verify result displays "test | 1" in table format
3. **test_cli_table_output_formatting**: Create table, insert 5 rows, SELECT them, verify formatted table output with proper column alignment and borders
4. **test_cli_json_output_format**: Launch CLI with `--json` flag, execute SELECT query, verify output is valid JSON array with row objects
5. **test_cli_csv_output_format**: Launch CLI with `--csv` flag, execute SELECT query, verify output is comma-separated with header row
6. **test_cli_show_tables_command**: Create 3 tables, execute `SHOW TABLES;`, verify all table names appear in output
7. **test_cli_describe_table_command**: Create table with multiple columns, execute `DESCRIBE table_name;`, verify schema details (name, type, nullable) displayed
8. **test_cli_websocket_subscription**: Create messages table, start CLI subscription with `SUBSCRIBE TO messages;`, insert message in separate thread, verify CLI displays live update
9. **test_cli_subscription_with_filter**: Subscribe with `SUBSCRIBE TO messages WHERE user_id='jamal';`, insert messages for different users, verify only matching messages displayed
10. **test_cli_subscription_cancel**: Start subscription, press Ctrl+C (simulate SIGINT), verify subscription stops and prompt returns
11. **test_cli_subscription_pause_resume**: Start subscription, type `\pause`, verify streaming stops, type `\continue`, verify streaming resumes
12. **test_cli_config_file_creation**: Delete `~/.kalam/config.toml`, launch CLI with connection params, verify config file created with provided values
13. **test_cli_config_file_loading**: Create config file with connection details, launch CLI without params, verify connection uses config values
14. **test_cli_connection_to_multiple_hosts**: Launch CLI with `-h http://host1`, execute query, then use `\connect -h http://host2`, verify connection switches
15. **test_cli_help_command**: Execute `\help`, verify output includes list of SQL commands and backslash commands
16. **test_cli_quit_commands**: Execute `\quit`, verify CLI exits with code 0 and no errors
17. **test_cli_jwt_authentication**: Launch CLI with `--token <valid_jwt>`, execute query, verify authentication succeeds
18. **test_cli_invalid_token_error**: Launch CLI with `--token invalid`, verify error message indicates authentication failure
19. **test_cli_localhost_bypass_mode**: Configure server for localhost bypass, launch CLI from localhost without token, verify queries execute as default user
20. **test_cli_batch_file_execution**: Create `test.sql` with 3 queries, execute `kalam-cli --file test.sql`, verify all queries run and output displays
21. **test_cli_syntax_error_handling**: Execute invalid SQL `SELEC * FROM;`, verify error message displays with helpful context
22. **test_cli_connection_failure_handling**: Launch CLI with invalid host `http://nonexistent:9999`, verify clear connection error message
23. **test_cli_flush_command**: Insert data into table, execute `\flush`, verify flush operation completes and displays status
24. **test_cli_health_check_command**: Execute `\health`, verify server health status displays with uptime and version info
25. **test_cli_color_output_toggle**: Launch with `--color=true`, execute query, verify ANSI color codes in output; launch with `--color=false`, verify no color codes
26. **test_cli_subscription_last_rows**: Subscribe with "last_rows" option in config, verify initial data fetch before streaming begins
27. **test_cli_multiple_sessions**: Launch 2 CLI instances concurrently, execute queries in both, verify sessions are isolated
28. **test_cli_session_timeout_handling**: Establish connection, wait beyond session timeout, execute query, verify reconnection or clear timeout error
29. **test_cli_interactive_history**: Execute 3 queries, press UP arrow key, verify previous queries are accessible via history
30. **test_cli_autocomplete_select**: Type "SEL" and press TAB, verify auto-completion to "SELECT"
31. **test_cli_autocomplete_multiple_matches**: Type "CRE" and press TAB, verify suggestions show "CREATE"
32. **test_cli_autocomplete_sql_keywords**: Press TAB on empty line, verify list includes SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, SHOW, DESCRIBE
33. **test_kalam_link_independent_usage**: Use kalam-link crate directly (without CLI) to execute query, verify connection and query execution work independently
34. **test_kalam_link_websocket_subscription**: Use kalam-link crate directly to establish WebSocket subscription, verify events are received

---

### CLI Functional Requirements (Embedded)

#### Project Structure and Architecture

- **FR-CLI-001**: CLI project MUST be located in `/cli` folder at the repository root (same level as `/backend`)
- **FR-CLI-002**: CLI project MUST consist of two crates: `kalam-link` (connection library) and `kalam-cli` (interactive terminal)
- **FR-CLI-003**: `kalam-link` crate MUST be a standalone library providing all connection, authentication, query execution, and subscription functionality
- **FR-CLI-004**: `kalam-link` MUST be designed to compile to WebAssembly for future use in browser-based Rust SDK
- **FR-CLI-005**: `kalam-link` MUST NOT depend on terminal-specific libraries or CLI-specific functionality
- **FR-CLI-006**: `kalam-cli` MUST depend on `kalam-link` for all database communication logic
- **FR-CLI-007**: `kalam-cli` MUST contain ONLY user interface, terminal rendering, command parsing, and output formatting logic
- **FR-CLI-008**: `kalam-cli` MUST NOT contain any direct HTTP request or WebSocket connection code (all via `kalam-link`)

#### Command-Line Interface and Configuration

- **FR-CLI-009**: CLI MUST accept command-line flags: `-u/--user`, `-h/--host`, `--token`, `--apikey`, `--json`, `--csv`, `--color`, `--file`
- **FR-CLI-010**: CLI MUST NOT support `--tenant` flag (multi-tenancy not required)
- **FR-CLI-011**: CLI MUST create a default configuration file at `~/.kalam/config.toml` on first run if it doesn't exist
- **FR-CLI-012**: Configuration file MUST support sections: `[connection]` (host, user, token) and `[output]` (format, color)
- **FR-CLI-013**: Configuration file MUST NOT include tenant_id field
- **FR-CLI-014**: CLI MUST display a welcome message on successful connection showing username and server URL (no tenant)
- **FR-CLI-015**: CLI MUST display an interactive prompt in the format: `kalam>` for user input

#### Query Execution and Output Formatting

- **FR-CLI-016**: CLI MUST delegate all SQL query execution to `kalam-link` crate's query execution methods
- **FR-CLI-017**: CLI MUST format query results as ASCII tables by default with aligned columns and borders
- **FR-CLI-018**: CLI MUST support `--json` flag to output query results as JSON arrays
- **FR-CLI-019**: CLI MUST support `--csv` flag to output query results as comma-separated values with header row
- **FR-CLI-020**: CLI MUST support `SHOW TABLES;` command delegating to `kalam-link` for execution
- **FR-CLI-021**: CLI MUST support `DESCRIBE <table>;` command delegating to `kalam-link` for execution

#### WebSocket Subscriptions and Live Queries

- **FR-CLI-022**: CLI MUST support `SUBSCRIBE TO <table> WHERE <condition>;` command
- **FR-CLI-023**: All WebSocket subscription logic MUST be handled by `kalam-link` crate
- **FR-CLI-024**: `kalam-link` MUST provide callback or async stream interface for receiving subscription events
- **FR-CLI-025**: CLI MUST render subscription events in real-time with timestamps and change type indicators
- **FR-CLI-026**: CLI MUST support Ctrl+C to gracefully stop active subscriptions (cleanup via `kalam-link`)

#### Interactive Commands and Auto-completion

- **FR-CLI-027**: CLI MUST support backslash commands: `\quit`, `\q`, `\help`, `\connect`, `\config`, `\flush`, `\health`, `\pause`, `\continue`
- **FR-CLI-028**: CLI MUST support TAB key for auto-completion of SQL keywords
- **FR-CLI-029**: Auto-completion MUST suggest SQL keywords: SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, SHOW, DESCRIBE, SUBSCRIBE, FROM, WHERE, ORDER BY, GROUP BY, LIMIT, OFFSET, JOIN, INNER, LEFT, RIGHT, OUTER
- **FR-CLI-030**: Auto-completion MUST match partial input (e.g., "SEL" + TAB → "SELECT")
- **FR-CLI-031**: When multiple keywords match, CLI MUST display a list of suggestions
- **FR-CLI-032**: CLI MUST support `\quit` and `\q` commands to exit the application gracefully
- **FR-CLI-033**: CLI MUST support `\help` command to display all available SQL and backslash commands
- **FR-CLI-034**: CLI MUST support `\connect` command to switch connection to a different host or user
- **FR-CLI-035**: CLI MUST support `\config` command to display current session configuration
- **FR-CLI-036**: CLI MUST support `\flush` command delegating to `kalam-link` to trigger table flush operations
- **FR-CLI-037**: CLI MUST support `\health` command delegating to `kalam-link` to query server health
- **FR-CLI-038**: CLI MUST support `\pause` and `\continue` commands to control active subscription streaming

#### Batch Execution and File Handling

- **FR-CLI-039**: CLI MUST support `--file <path>` flag to execute SQL queries from a file in batch mode
- **FR-CLI-040**: Batch file execution MUST process queries sequentially using `kalam-link` and display results for each query

#### Authentication and Security

- **FR-CLI-041**: All authentication logic MUST be implemented in `kalam-link` crate
- **FR-CLI-042**: `kalam-link` MUST support JWT token authentication via token parameter
- **FR-CLI-043**: `kalam-link` MUST support API key authentication via apikey parameter
- **FR-CLI-044**: `kalam-link` MUST include `X-USER-ID` header in all API requests based on provided user_id
- **FR-CLI-045**: `kalam-link` MUST NOT include `X-TENANT-ID` header (multi-tenancy not supported)
- **FR-CLI-046**: CLI MUST display clear error messages for authentication failures received from `kalam-link`

#### Error Handling and User Experience

- **FR-CLI-047**: CLI MUST display clear error messages for connection failures returned from `kalam-link`
- **FR-CLI-048**: CLI MUST display clear error messages for query syntax errors returned from `kalam-link`
- **FR-CLI-049**: CLI MUST implement readline-like functionality with command history and arrow key navigation
- **FR-CLI-050**: CLI MUST persist command history across sessions in `~/.kalam/history`

#### Technical Stack and Dependencies

- **FR-CLI-051**: `kalam-link` MUST use `tokio` for async runtime
- **FR-CLI-052**: `kalam-link` MUST use `reqwest` for HTTP requests
- **FR-CLI-053**: `kalam-link` MUST use `tungstenite` or `tokio-tungstenite` for WebSocket connections
- **FR-CLI-054**: `kalam-link` MUST be compatible with WebAssembly compilation target (wasm32-unknown-unknown)
- **FR-CLI-055**: `kalam-cli` MUST use `ratatui` or `crossterm` for terminal UI rendering
- **FR-CLI-056**: `kalam-cli` MUST use `rustyline` or similar for readline functionality and command history
- **FR-CLI-057**: `kalam-cli` MUST use `toml` crate for configuration file parsing
- **FR-CLI-058**: `kalam-cli` MUST use `tabled` or `prettytable-rs` for ASCII table formatting
- **FR-CLI-059**: `kalam-cli` MUST use `clap` for command-line argument parsing

#### Connection Management and Resilience

- **FR-CLI-060**: `kalam-link` MUST handle connection timeouts and implement retry logic for network failures
- **FR-CLI-061**: `kalam-link` MUST provide health check method for validating server connectivity
- **FR-CLI-062**: `kalam-link` MUST support connection pooling or reuse for multiple sequential queries
- **FR-CLI-063**: CLI MUST validate configuration values and provide helpful error messages for invalid config

#### Output Formatting and Display

- **FR-CLI-064**: CLI MUST support configurable color output via `--color` flag and config file (true/false)
- **FR-CLI-065**: CLI MUST handle terminal resize events gracefully during table rendering
- **FR-CLI-066**: CLI MUST paginate large result sets automatically (default: 1000 rows per page)
- **FR-CLI-067**: CLI MUST display "Press Enter for more..." prompt for paginated results

---

### User Story 1 - Parametrized Query Execution with Caching (Priority: P1)

Database users need to execute queries efficiently with dynamic parameters while maintaining security and performance. The system should compile queries once and reuse the execution plan for subsequent calls with different parameter values.

**Why this priority**: Query compilation is expensive. Eliminating repeated compilation for the same query structure will significantly improve response times and reduce CPU usage. This is a fundamental performance optimization that benefits all database operations.

**Independent Test**: Can be fully tested by submitting a parametrized query via the `/api/sql` endpoint, verifying it executes correctly with parameters, then submitting the same query with different parameters and confirming the cached execution plan is used (observable through faster execution time and query plan inspection).

**Acceptance Scenarios**:

1. **Given** a user has a SQL query with dynamic values, **When** they submit `{ "sql": "SELECT * FROM messages WHERE user_id = $1 AND created_at > $2", "params": ["user123", "2025-01-01"] }` to `/api/sql`, **Then** the query executes successfully and returns filtered results
2. **Given** a parametrized query has been executed once, **When** the same query structure is submitted again with different parameter values, **Then** the cached execution plan is used without recompilation
3. **Given** a query with invalid parameter count, **When** submitted to the API, **Then** the system returns a clear error message indicating parameter mismatch
4. **Given** query results are being returned, **When** the query execution configuration enables timing, **Then** the response includes the query execution duration

**Integration Tests** (backend/tests/integration/test_parametrized_queries.rs):

1. **test_parametrized_query_execution**: Create user table, execute parametrized SELECT with $1, $2 placeholders, verify results match parameter values
2. **test_execution_plan_caching**: Execute same query structure twice with different parameters, verify second execution is faster (plan cached)
3. **test_parameter_count_mismatch**: Submit query with 2 placeholders but 1 parameter value, verify error message includes "parameter mismatch"
4. **test_parameter_type_validation**: Submit parametrized INSERT with wrong type (string for INT column), verify type error returned
5. **test_query_timing_in_response**: Enable timing in config, execute parametrized query, verify response includes execution_time_ms field
6. **test_parametrized_insert_update_delete**: Test parametrized INSERT, UPDATE, DELETE operations with parameter substitution
7. **test_concurrent_parametrized_queries**: Execute multiple parametrized queries concurrently, verify no cache contention or errors

---

### User Story 2 - Automatic Table Flushing with Scheduled Jobs (Priority: P1)

Database administrators need user table data automatically persisted to storage at configured intervals without manual intervention. The system should group data by user, apply configured sharding strategies, and write to organized storage paths.

**Why this priority**: Data durability is critical. Without automatic flushing, data remains only in memory/buffer and is vulnerable to loss. This is essential for production readiness and data reliability.

**Independent Test**: Can be fully tested by creating a table with flush configuration, inserting data from multiple users, waiting for the scheduled flush interval, then verifying Parquet files are created in the correct storage locations organized by user and shard.

**Acceptance Scenarios**:

1. **Given** a table is created with flush interval configuration, **When** the scheduled flush time arrives and data exists in the buffer, **Then** a flush job initiates automatically
2. **Given** multiple users have data in a table buffer, **When** automatic flush executes, **Then** data is grouped by user_id and written to separate storage locations
3. **Given** flush storage locations are configured with path templates, **When** data is flushed, **Then** files are written following the template pattern (e.g., `{storageLocation}/{namespace}/users/{userId}/{tableName}/`)
4. **Given** a sharding strategy is configured, **When** data is flushed, **Then** data is distributed across shards according to the configured function
5. **Given** flush configuration specifies separate paths for user tables vs shared tables, **When** flush executes for each table type, **Then** data is written to the appropriate directory structure

**Integration Tests** (backend/tests/integration/test_automatic_flushing.rs):

1. **test_scheduled_flush_interval**: Create table with 5-second flush interval, insert data, wait for scheduler, verify Parquet files created at storage location
2. **test_multi_user_flush_grouping**: Insert data from user1 and user2, trigger flush, verify separate Parquet files at {storageLocation}/users/user1/ and /users/user2/
3. **test_storage_path_template_substitution**: Create table with template path containing {namespace}, {userId}, {tableName}, flush data, verify actual paths match substituted template
4. **test_sharding_strategy_distribution**: Configure alphabetic sharding (a-z), insert data across multiple shards, flush, verify files distributed to correct shard directories
5. **test_user_vs_shared_table_paths**: Create user table and shared table, insert data, flush both, verify user data at users/{userId}/ and shared data at {namespace}/{table}/
6. **test_flush_job_status_tracking**: Trigger flush, query system.jobs table, verify flush job recorded with status, metrics, and storage location
7. **test_scheduler_recovery_after_restart**: Insert data, shutdown server before flush, restart, verify scheduler triggers pending flush

---

### User Story 3 - Manual Table Flushing via SQL Command (Priority: P2)

Database administrators need to manually trigger immediate table flushing for maintenance, backup, or server shutdown scenarios. The command should provide control over which tables to flush and confirmation of the operation.

**Why this priority**: Manual control is necessary for planned maintenance and backup operations. While automatic flushing handles routine operations, administrators need the ability to force immediate persistence.

**Independent Test**: Can be fully tested by executing a `FLUSH TABLE` SQL command via the API, then verifying the specified table's buffered data is immediately written to storage and the buffer is cleared.

**Acceptance Scenarios**:

1. **Given** a user table has buffered data, **When** administrator executes `FLUSH TABLE namespace.table_name`, **Then** all buffered data is immediately written to storage
2. **Given** multiple tables exist, **When** administrator executes `FLUSH ALL TABLES`, **Then** all tables with buffered data are flushed sequentially
3. **Given** a flush operation completes, **When** the SQL command returns, **Then** the response includes the number of records flushed and the target storage location
4. **Given** the server is shutting down, **When** the shutdown sequence initiates, **Then** automatic flush of all tables executes before the process terminates

**Integration Tests** (backend/tests/integration/test_manual_flushing.rs):

1. **test_flush_table_command**: Create user table, insert 100 rows, execute FLUSH TABLE namespace.table_name, verify Parquet file created and buffer cleared
2. **test_flush_all_tables_command**: Create 3 tables with buffered data, execute FLUSH ALL TABLES, verify all tables flushed and response includes count for each table
3. **test_flush_response_includes_metrics**: Execute FLUSH TABLE, verify response includes records_flushed count and storage_location path
4. **test_concurrent_flush_prevention**: Trigger FLUSH TABLE in one thread, attempt FLUSH same table in another thread, verify second request queued/rejected with appropriate message
5. **test_flush_empty_table**: Execute FLUSH TABLE on table with no buffered data, verify response indicates 0 records flushed with success status
6. **test_shutdown_automatic_flush**: Insert data into multiple tables, initiate server shutdown, verify all tables flushed before process terminates (check Parquet files exist)
7. **test_flush_table_synchronous_operation**: Execute FLUSH TABLE, measure response time, verify command blocks until flush completes (not async fire-and-forget)

---

### User Story 4 - Session-Level Table Registration Caching (Priority: P2)

Database users who repeatedly query their own tables should experience faster query execution through intelligent table registration caching. The system should maintain frequently-accessed table registrations in memory and automatically evict unused registrations.

**Why this priority**: Current architecture registers/unregisters tables per query, creating overhead. Session-level caching eliminates this repeated work for sequential queries against the same tables, significantly improving user experience for interactive workloads.

**Independent Test**: Can be fully tested by executing multiple queries against a user table in the same session, measuring execution time, and verifying subsequent queries execute faster due to cached table registration (observable through query timing and session cache inspection).

**Acceptance Scenarios**:

1. **Given** a user queries their table for the first time in a session, **When** the query executes, **Then** the table is registered and the registration is cached in the session context
2. **Given** a table registration exists in the session cache, **When** a subsequent query references the same table, **Then** the cached registration is used without re-registration
3. **Given** a user session has multiple cached table registrations, **When** tables remain unused beyond a configured timeout, **Then** those registrations are automatically evicted from the cache
4. **Given** a table's schema is modified, **When** a query attempts to use a cached registration, **Then** the system detects the schema change and re-registers the table with the updated schema

**Integration Tests** (backend/tests/integration/test_session_caching.rs):

1. **test_first_query_caches_registration**: Create user table, execute SELECT query, measure execution time, execute same SELECT again, verify second query is faster (cached registration)
2. **test_cached_registration_reuse**: Execute 10 sequential queries on same table in one session, verify only first query performs registration (inspect debug logs or metrics)
3. **test_cache_eviction_after_timeout**: Configure short cache timeout (30s), query table, wait beyond timeout, query again, verify re-registration occurred
4. **test_schema_change_invalidates_cache**: Query table, execute ALTER TABLE ADD COLUMN, query table again, verify cache invalidated and new schema loaded
5. **test_multi_table_session_cache**: Create 5 tables, query all 5 in sequence, query all 5 again, verify cached registrations for all tables (faster second round)
6. **test_cache_isolation_between_sessions**: Query table in session1, query same table in session2, verify each session maintains independent cache
7. **test_dropped_table_cache_cleanup**: Query table, DROP TABLE, attempt query again, verify cache entry removed and appropriate error returned

---

### User Story 5 - Namespace Validation for Table Creation (Priority: P2)

Database users should be prevented from creating tables in non-existent namespaces. The system must validate namespace existence before allowing any table creation operation.

**Why this priority**: Data integrity and organizational structure depend on proper namespace management. Allowing table creation in non-existent namespaces leads to orphaned data and confusing error states.

**Independent Test**: Can be fully tested by attempting to create a table with a non-existent namespace, verifying the operation fails with a clear error message, then creating the namespace and confirming the table creation succeeds.

**Acceptance Scenarios**:

1. **Given** a user attempts to create a table, **When** they specify a namespace that doesn't exist, **Then** the system returns an error: "Namespace 'X' does not exist. Create it first with CREATE NAMESPACE."
2. **Given** a namespace exists, **When** a user creates a table within that namespace, **Then** the table is successfully created
3. **Given** validation applies to all table types, **When** creating user, shared, or stream tables, **Then** namespace existence is validated for each type

**Integration Tests** (backend/tests/integration/test_namespace_validation.rs):

1. **test_create_table_nonexistent_namespace_error**: Attempt CREATE USER TABLE in namespace "nonexistent", verify error contains "Namespace 'nonexistent' does not exist"
2. **test_create_table_after_namespace_creation**: Attempt CREATE TABLE in nonexistent namespace (fails), CREATE NAMESPACE, retry CREATE TABLE (succeeds)
3. **test_user_table_namespace_validation**: Attempt CREATE USER TABLE without namespace, verify validation error with guidance message
4. **test_shared_table_namespace_validation**: Attempt CREATE SHARED TABLE in nonexistent namespace, verify same validation applies
5. **test_stream_table_namespace_validation**: Attempt CREATE STREAM TABLE in nonexistent namespace, verify same validation applies
6. **test_namespace_validation_race_condition**: Create namespace, immediately create table in concurrent thread, verify no race condition errors
7. **test_error_message_includes_guidance**: Attempt table creation in nonexistent namespace, verify error includes "Create it first with CREATE NAMESPACE" guidance

---

### User Story 6 - Code Quality and Maintenance Improvements (Priority: P3)

Development teams need a clean, maintainable codebase with reduced duplication, consistent patterns, and comprehensive documentation. The system should follow established architectural principles and use shared abstractions where appropriate.

**Why this priority**: Code quality improvements don't directly impact end users but significantly affect development velocity, bug rates, and long-term maintainability. These are important for sustainable development but can be addressed after core functionality is stable.

**Independent Test**: Can be verified through code review, measuring metrics like code duplication percentage, test coverage, documentation completeness, and adherence to architectural patterns defined in project guidelines.

**Acceptance Scenarios**:

1. **Given** multiple system table providers exist, **When** reviewing the codebase, **Then** they share a common base implementation eliminating duplication
2. **Given** table name constants are needed across crates, **When** examining the code, **Then** all table names are defined once in a shared location (e.g., kalamdb-commons)
3. **Given** type-safe wrappers exist (NamespaceId, TableName), **When** reviewing function signatures, **Then** they consistently use these types instead of raw strings
4. **Given** critical functions like scan() exist, **When** reviewing code, **Then** they have comprehensive documentation explaining their purpose, parameters, and usage patterns
5. **Given** repeated string formatting patterns exist (e.g., column family naming), **When** examining the code, **Then** they use centralized helper functions instead of inline formatting
6. **Given** the project uses external dependencies, **When** performing maintenance, **Then** all crate dependencies are updated to their latest compatible versions
7. **Given** the README documentation exists, **When** reviewing it, **Then** it accurately reflects current architecture with minimal Parquet-specific mentions and includes WebSocket information
8. **Given** DDL-related code exists across crates, **When** reviewing architecture, **Then** DDL definitions are consolidated in kalamdb-sql where they logically belong
9. **Given** storage operations use RocksDB, **When** reviewing direct usage, **Then** kalamdb-sql accesses storage through kalamdb-store abstraction layer instead of direct RocksDB calls
10. **Given** system tables need a catalog, **When** querying system tables, **Then** they use "system" as the default catalog consistently
11. **Given** test suites exist, **When** running tests, **Then** they support configuration to run against either local server or temporary test server
12. **Given** a kalamdb-commons crate is needed, **When** reviewing crate structure, **Then** shared models (UserId, NamespaceId, TableName), system helpers, error types, and configuration models are consolidated in kalamdb-commons
13. **Given** testing and development dependencies exist, **When** building release binaries, **Then** test-only libraries are excluded from the final binary to minimize size
14. **Given** live query subscriptions need management, **When** reviewing architecture, **Then** a separate kalamdb-live crate handles subscription logic and communication with kalamdb-store and kalamdb-sql
15. **Given** live query filtering uses expressions, **When** implementing filter checks, **Then** DataFusion expression objects are used and cached for performance
16. **Given** SQL functions are needed, **When** implementing custom functions, **Then** DataFusion's built-in function infrastructure is leveraged where possible

**Integration Tests** (backend/tests/integration/test_code_quality.rs):

1. **test_system_table_providers_use_common_base**: Verify all system table providers inherit from common base implementation (code inspection/reflection test)
2. **test_type_safe_wrappers_usage**: Create tables using type-safe wrappers (NamespaceId, TableName), verify operations succeed without raw string errors
3. **test_column_family_helper_functions**: Verify column family names generated through centralized helpers match expected patterns
4. **test_kalamdb_commons_models_accessible**: Import and use UserId, NamespaceId, TableName from kalamdb-commons crate in integration test
5. **test_system_catalog_consistency**: Query system tables, verify all use "system" catalog prefix consistently
6. **test_local_vs_temporary_server_config**: Run subset of tests against local server (if available) and temporary server, verify both work
7. **test_binary_size_optimization**: Build release binary, verify test-only dependencies not included (check binary size is within limits)

---

### User Story 7 - Storage Backend Abstraction and Architecture Cleanup (Priority: P3)

Development teams need the ability to support alternative storage backends beyond RocksDB while maintaining consistent APIs. The system should abstract storage operations to allow pluggable backends like Sled, Redis, or others in the future.

**Why this priority**: Storage backend flexibility is important for future scalability and deployment options, but doesn't block current functionality. This architectural improvement enables future features without requiring large rewrites.

**Independent Test**: Can be verified by implementing a storage trait/interface, migrating RocksDB to use this interface, and demonstrating that storage operations work identically through the abstraction layer.

**Acceptance Scenarios**:

1. **Given** storage operations are needed, **When** reviewing the architecture, **Then** a storage backend trait/interface defines all required operations
2. **Given** RocksDB is the current backend, **When** examining implementations, **Then** RocksDB operations implement the storage trait without exposing RocksDB-specific details
3. **Given** system tables have a naming convention, **When** renaming occurs, **Then** "system.storage_locations" is renamed to "system.storages" consistently across all code and documentation
4. **Given** column families are used for organization, **When** considering alternative backends, **Then** the abstraction layer provides equivalent partitioning mechanisms for non-RocksDB backends

**Integration Tests** (backend/tests/integration/test_storage_abstraction.rs):

1. **test_storage_trait_interface_exists**: Verify storage trait defines get, put, delete, scan, batch operations (code inspection test)
2. **test_rocksdb_implements_storage_trait**: Verify RocksDB backend implements storage trait without exposing RocksDB types in public API
3. **test_system_storages_table_renamed**: Query system.storages table, verify it exists and system.storage_locations does not (naming consistency)
4. **test_storage_operations_through_abstraction**: Perform insert/update/delete/select operations, verify they use storage abstraction layer (no direct RocksDB calls)
5. **test_column_family_abstraction**: Create multiple tables, verify column family concepts work through abstraction (prepare for non-RocksDB backends)
6. **test_alternative_backend_compatibility**: If alternative backend available (Sled/Redis), run basic CRUD tests through storage trait
7. **test_storage_backend_error_handling**: Trigger storage errors (disk full simulation), verify abstraction layer handles errors gracefully

---

### User Story 8 - Documentation Organization and Deployment Infrastructure (Priority: P3)

Users and operators need well-organized documentation with clear categories and containerized deployment options. Documentation should be easy to navigate with logical grouping, and deployment should be straightforward using Docker.

**Why this priority**: Good documentation and deployment infrastructure lower barriers to entry and improve operational efficiency. While not blocking development, these improvements significantly enhance user experience and production readiness.

**Independent Test**: Can be verified by reviewing the organized /docs folder structure with clear categories, building a Docker image successfully, and running the system via docker-compose with all services functional.

**Acceptance Scenarios**:

1. **Given** documentation files exist in /docs, **When** organizing them, **Then** they are categorized into logical subfolders: build/, quickstart/, architecture/
2. **Given** outdated or redundant documentation exists, **When** cleaning up /docs, **Then** unnecessary files are removed while preserving essential information
3. **Given** users need to run KalamDB in containers, **When** Docker files are created, **Then** a complete Dockerfile exists in /docker folder that builds a working image
4. **Given** a Dockerfile exists, **When** building the image, **Then** the resulting container includes the server binary and required dependencies
5. **Given** deployment scenarios exist, **When** providing orchestration, **Then** a docker-compose.yml in /docker folder enables single-command system startup
6. **Given** docker-compose configuration exists, **When** running the system, **Then** all services (database server, storage volumes, networking) are properly configured

**Integration Tests** (backend/tests/integration/test_documentation_and_deployment.rs):

1. **test_docs_folder_organization**: Verify /docs contains build/, quickstart/, architecture/ subfolders with no orphan files in root
2. **test_dockerfile_builds_successfully**: Run docker build on /docker/Dockerfile, verify image builds without errors
3. **test_docker_image_starts_server**: Build Docker image, run container, verify server starts and responds to health check endpoint
4. **test_docker_compose_brings_up_stack**: Execute docker-compose up, verify all services start (database, volumes, networking)
5. **test_docker_container_environment_variables**: Start container with custom env vars (config overrides), verify server uses provided configuration
6. **test_docker_volume_persistence**: Start container, create namespace/table, stop container, restart with same volumes, verify data persists
7. **test_docker_image_size_within_limits**: Build Docker image, verify size is under 100MB (excluding data volumes)

---

### User Story 9 - Enhanced API Features and Live Query Improvements (Priority: P2)

Database users need more flexible API capabilities including batch SQL execution, enhanced live query features, and improved system observability. These enhancements build on the base functionality to provide better developer experience and operational control.

**Why this priority**: These are quality-of-life improvements that enhance developer productivity and operational capabilities without changing core architecture. They address common patterns and pain points discovered during usage.

**Independent Test**: Can be fully tested by submitting batch SQL requests, creating WebSocket subscriptions with initial data fetch, monitoring enhanced system tables, and executing administrative commands.

**Acceptance Scenarios**:

1. **Given** a user needs to execute multiple related SQL commands, **When** they submit a request with semicolon-separated statements to `/api/sql`, **Then** all statements execute in sequence and individual results are returned
2. **Given** a user establishes a WebSocket subscription, **When** they specify "last_rows": N in subscription options, **Then** they immediately receive the last N rows before real-time updates begin
3. **Given** a table has active live query subscriptions, **When** an administrator attempts to DROP TABLE, **Then** the operation fails with error listing the active subscription count
4. **Given** an administrator needs to terminate a subscription, **When** they execute `KILL LIVE QUERY <live_id>`, **Then** the specified subscription is disconnected and removed from system.live_queries
5. **Given** system.live_queries exists, **When** queried, **Then** it includes options (JSON), changes counter, and node identifier fields
6. **Given** system.jobs exists, **When** queried, **Then** it includes parameters array, result string, trace string, and resource metrics (memory_used, cpu_used)
7. **Given** users query tables, **When** DESCRIBE TABLE is executed, **Then** the output includes current schema version and reference to schema history in system.table_schemas
8. **Given** administrators monitor tables, **When** SHOW TABLE STATS is executed, **Then** row counts, storage size, and buffer status are displayed

**Integration Tests** (backend/tests/integration/test_enhanced_api_features.rs):

1. **test_batch_sql_execution**: Submit request with 3 semicolon-separated SQL statements, verify all execute in sequence with individual results returned
2. **test_batch_sql_partial_failure**: Submit batch with one invalid statement, verify execution stops at failure point with clear error indicating which statement failed
3. **test_websocket_initial_data_fetch**: Create table, insert 100 rows, subscribe with "last_rows": 50, verify immediate response with 50 most recent rows
4. **test_drop_table_with_active_subscriptions**: Create WebSocket subscription, attempt DROP TABLE, verify error includes active subscription count
5. **test_kill_live_query_command**: Create subscription, query system.live_queries for live_id, execute KILL LIVE QUERY, verify subscription disconnected
6. **test_system_live_queries_enhanced_fields**: Create subscription with options, query system.live_queries, verify options (JSON), changes counter, node fields populated
7. **test_system_jobs_enhanced_fields**: Trigger flush job, query system.jobs, verify parameters, result, trace, memory_used, cpu_used fields populated
8. **test_describe_table_schema_history**: Create table, ALTER TABLE twice, DESCRIBE TABLE, verify output includes current_schema_version and history reference
9. **test_show_table_stats_command**: Insert data, flush, execute SHOW TABLE STATS, verify output includes buffered/flushed row counts, storage size, last flush timestamp
10. **test_shared_table_subscription_prevention**: Create shared table, attempt WebSocket subscription, verify error "Live query subscriptions not supported on shared tables"

---

### User Story 10 - User Management SQL Commands (Priority: P2)

Database administrators need SQL commands to manage users in the system.users table for user registration, updates, and removal. Standard SQL syntax should be used for consistency with existing table operations.

**Why this priority**: User management is a fundamental administrative task. While the system.users table exists, providing standard SQL commands (INSERT/UPDATE/DELETE) makes user administration consistent with other database operations and easier for administrators familiar with SQL.

**Independent Test**: Can be fully tested by executing INSERT USER, UPDATE USER, and DELETE USER SQL commands via the `/api/sql` endpoint, then querying system.users to verify changes were persisted correctly.

**Acceptance Scenarios**:

1. **Given** an administrator needs to add a user, **When** they execute `INSERT INTO system.users (user_id, username, metadata) VALUES ('user123', 'john_doe', '{"role": "admin"}')`, **Then** the user is created in system.users table
2. **Given** an administrator needs to update user information, **When** they execute `UPDATE system.users SET username = 'jane_doe', metadata = '{"role": "user"}' WHERE user_id = 'user123'`, **Then** the user record is updated
3. **Given** an administrator needs to remove a user, **When** they execute `DELETE FROM system.users WHERE user_id = 'user123'`, **Then** the user is removed from system.users table
4. **Given** a user_id already exists, **When** an administrator attempts to INSERT with the same user_id, **Then** the system returns error "User with user_id 'X' already exists"
5. **Given** an administrator updates a non-existent user, **When** UPDATE is executed, **Then** the system returns error "User with user_id 'X' not found"
6. **Given** metadata is provided in INSERT/UPDATE, **When** the SQL executes, **Then** JSON metadata is validated and stored correctly
7. **Given** an administrator queries users, **When** they execute `SELECT * FROM system.users WHERE username LIKE '%john%'`, **Then** matching users are returned with all fields (user_id, username, metadata, created_at, updated_at)

**Integration Tests** (backend/tests/integration/test_user_management_sql.rs):

1. **test_insert_user_into_system_users**: Execute INSERT INTO system.users with user_id, username, metadata, verify user created and queryable
2. **test_update_user_in_system_users**: Insert user, execute UPDATE to modify username and metadata, verify changes persisted
3. **test_delete_user_from_system_users**: Insert user, execute DELETE FROM system.users, query to verify user removed
4. **test_duplicate_user_id_validation**: Insert user with user_id "user123", attempt INSERT with same user_id, verify error "User with user_id 'user123' already exists"
5. **test_update_nonexistent_user_error**: Execute UPDATE for user_id that doesn't exist, verify error "User with user_id 'X' not found"
6. **test_json_metadata_validation**: Insert user with malformed JSON metadata '{"invalid}', verify error indicates JSON validation failure
7. **test_automatic_timestamps**: Insert user, verify created_at set automatically; UPDATE user, verify updated_at changes to current time
8. **test_partial_update_preserves_fields**: Insert user with username and metadata, UPDATE only username, verify metadata unchanged
9. **test_required_fields_validation**: Attempt INSERT without user_id or username, verify NOT NULL constraint error
10. **test_select_with_filtering**: Insert multiple users, execute SELECT with WHERE username LIKE filter, verify only matching users returned

---

### User Story 11 - Live Query Change Detection Integration Testing (Priority: P1)

Developers need to verify that live query subscriptions correctly detect and deliver all data changes (INSERT, UPDATE, DELETE) in real-time across concurrent operations. The system must handle realistic scenarios like AI agents writing messages while multiple clients are listening.

**Why this priority**: Live queries are a core feature of KalamDB. Comprehensive integration testing ensures the WebSocket subscription system reliably delivers all changes without loss, duplication, or ordering issues under concurrent load.

**Independent Test**: Can be fully tested by creating a messages table, establishing a WebSocket subscription in one thread, performing INSERT/UPDATE/DELETE operations from another thread, and verifying the listener receives all changes with correct change types and data.

**Acceptance Scenarios**:

1. **Given** a messages table exists with WebSocket subscription active, **When** INSERT operations occur from a separate thread, **Then** the listener receives all INSERT notifications with complete message data
2. **Given** an active subscription is listening for changes, **When** UPDATE operations modify existing messages, **Then** the listener receives UPDATE notifications with both old and new values
3. **Given** a subscription is monitoring messages, **When** DELETE operations soft-delete messages, **Then** the listener receives DELETE notifications with the deleted message data and _deleted=true
4. **Given** multiple concurrent writers insert messages simultaneously, **When** the listener monitors the table, **Then** all INSERT notifications are received without loss or duplication
5. **Given** a realistic AI scenario with agents writing messages, **When** human clients subscribe to conversation updates, **Then** all AI-generated messages are delivered in real-time with correct timestamps
6. **Given** mixed operations occur (INSERT, UPDATE, DELETE) in rapid succession, **When** monitored by a subscription, **Then** all changes are delivered in correct chronological order
7. **Given** a subscription has been active for extended duration, **When** the system.live_queries table is queried, **Then** the changes counter accurately reflects the total notifications delivered

**Integration Tests** (backend/tests/integration/test_live_query_changes.rs):

1. **test_live_query_detects_inserts**: Create messages table, start WebSocket subscription in spawned thread, INSERT 100 messages from main thread, verify listener receives all 100 INSERT notifications
2. **test_live_query_detects_updates**: Subscribe to messages table, INSERT 50 messages, UPDATE all 50 messages, verify listener receives 50 INSERT + 50 UPDATE notifications with old/new values
3. **test_live_query_detects_deletes**: Subscribe to messages, INSERT 30 messages, DELETE 15 messages (soft delete), verify listener receives 30 INSERT + 15 DELETE notifications with _deleted=true
4. **test_concurrent_writers_no_message_loss**: Create 5 writer threads each inserting 20 messages concurrently, verify single listener receives all 100 messages without loss or duplication
5. **test_ai_message_scenario**: Simulate AI agent writing messages (INSERT with AI metadata), human client subscribing to conversation_id filter, verify all AI messages delivered in real-time
6. **test_mixed_operations_ordering**: Perform sequence: INSERT msg1, UPDATE msg1, INSERT msg2, DELETE msg1, verify listener receives changes in exact order
7. **test_changes_counter_accuracy**: Subscribe to table, trigger 50 changes (INSERT/UPDATE/DELETE), query system.live_queries, verify changes field = 50
8. **test_multiple_listeners_same_table**: Create 3 concurrent WebSocket subscriptions to same table, INSERT 20 messages, verify each listener receives all 20 notifications independently
9. **test_listener_reconnect_no_data_loss**: Subscribe to table, INSERT 10 messages, disconnect/reconnect WebSocket, INSERT 10 more messages, verify no messages lost during reconnection
10. **test_high_frequency_changes**: INSERT 1000 messages as fast as possible, verify listener receives all 1000 notifications with correct sequence numbers

---

### User Story 12 - Memory Leak and Performance Stress Testing (Priority: P1)

Operations teams need confidence that the system handles sustained high load without memory leaks, resource exhaustion, or performance degradation. The system must maintain stability under concurrent writers, high insert rates, and multiple active subscriptions.

**Why this priority**: Production stability requires verification that the system doesn't accumulate memory, leak connections, or degrade under load. Stress testing identifies resource management issues before production deployment.

**Independent Test**: Can be fully tested by spawning 10 concurrent writer threads performing continuous inserts, 20 concurrent WebSocket subscriptions listening for changes, running for extended duration (5+ minutes), and monitoring memory usage, CPU utilization, and WebSocket connection stability.

**Acceptance Scenarios**:

1. **Given** 10 concurrent writer threads continuously insert data, **When** the system runs for 5 minutes, **Then** memory usage remains stable without continuous growth indicating leaks
2. **Given** 20 active WebSocket subscriptions are monitoring a table, **When** high-frequency inserts occur (1000+ rows/second), **Then** all subscriptions receive notifications without dropping connections
3. **Given** sustained write load from multiple threads, **When** monitoring system resources, **Then** CPU usage stays within reasonable limits (< 80% on average) and responds to queries
4. **Given** long-running stress test with writers and listeners, **When** checking WebSocket connections, **Then** no connections are leaked or left in zombie state
5. **Given** extreme load with 10 writers and 20 listeners, **When** monitoring memory at 1-minute intervals, **Then** memory usage stabilizes and does not grow linearly with time
6. **Given** the system is under stress, **When** normal queries are executed, **Then** query response times remain within acceptable limits (< 500ms for simple SELECT)
7. **Given** stress test completes and all threads terminate, **When** checking system resources, **Then** memory is properly released and returns to baseline levels

**Integration Tests** (backend/tests/integration/test_stress_and_memory.rs):

1. **test_memory_stability_under_write_load**: Spawn 10 writer threads inserting 10,000 rows each, measure memory every 30 seconds, verify memory growth < 10% over baseline
2. **test_concurrent_writers_and_listeners**: Start 10 writers + 20 WebSocket listeners, run for 5 minutes, verify no WebSocket disconnections and all messages delivered
3. **test_cpu_usage_under_load**: Run sustained write load (1000 inserts/sec), measure CPU usage, verify average < 80% and system remains responsive
4. **test_websocket_connection_leak_detection**: Create 50 WebSocket subscriptions, close 25, verify server properly releases connections (check via system.live_queries and netstat)
5. **test_memory_release_after_stress**: Run heavy load test, stop all writers/listeners, wait 60 seconds, verify memory returns to within 5% of baseline
6. **test_query_performance_under_stress**: While stress test runs (10 writers, 20 listeners), execute SELECT queries, verify response times < 500ms at p95
7. **test_flush_operations_during_stress**: Run stress test with continuous writes, trigger periodic manual flushes, verify no memory accumulation from unflushed buffers
8. **test_actor_system_stability**: Monitor actor system (flush jobs, live query actors) during stress test, verify no actor mailbox overflow or stuck actors
9. **test_rocksdb_memory_bounds**: Configure RocksDB memory limits, run stress test, verify RocksDB respects bounds and doesn't cause OOM
10. **test_graceful_degradation**: Gradually increase load until system reaches capacity, verify it degrades gracefully (slower responses) rather than crashing

---

### Edge Cases

- **CLI Edge Cases**:
  - What happens when CLI is launched without network connectivity to the server?
  - How does CLI handle server shutdown while a subscription is active?
  - What occurs when the terminal window is resized during table output rendering?
  - How does CLI handle very wide tables that exceed terminal width?
  - What happens when config file contains malformed TOML syntax?
  - How does CLI behave when `~/.kalam/` directory doesn't exist or isn't writable?
  - What occurs when command history file becomes corrupted?
  - How does CLI handle authentication token expiration during an active session?
  - What happens when a user executes a long-running query and tries to cancel with Ctrl+C?
  - How does CLI display query results with special characters or Unicode that the terminal doesn't support?
  - What occurs when multiple CLI instances try to write to the same config or history file simultaneously?
  - How does CLI handle WebSocket ping/pong timeout during an active subscription?
  - What happens when TAB is pressed with no partial input (empty line)?
  - How does auto-completion behave when multiple keywords share the same prefix (e.g., "CREATE" and "CREATE TABLE")?
  - What occurs when a user types a complete keyword and presses TAB again?

- **kalam-link Edge Cases**:
  - How does `kalam-link` handle API endpoint URL changes without code recompilation?
  - What happens when `kalam-link` receives malformed JSON from the server?
  - How does `kalam-link` handle WebSocket connection upgrade failures?
  - What occurs when a WebSocket connection is established but no ping/pong is received?
  - How does `kalam-link` behave in WebAssembly context when attempting to access file system or OS-specific features?
  - What happens when JWT token is valid but user_id in token doesn't exist on server?
  - How does `kalam-link` handle HTTP redirect responses?
  - What occurs when `kalam-link` is used concurrently from multiple threads?

- What happens when a parametrized query is submitted with a parameter count that doesn't match the placeholder count?
- How does the system handle flush operations when storage location is unavailable or disk space is exhausted?
- What occurs if a manual flush is triggered while an automatic flush is already in progress for the same table?
- How does table registration caching behave when a user's session spans a schema migration?
- What happens when attempting to create a table in a namespace that was deleted after validation but before table creation completes?
- How does the system handle queries against cached table registrations when the underlying table has been dropped?
- What occurs when sharding configuration changes while flush jobs are in progress?
- How does the system handle flush job scheduling when the server was offline during scheduled flush time?
- What happens when switching storage backends with existing data in RocksDB - is migration required?
- How does the storage abstraction handle backend-specific features (like RocksDB column families) when using backends that don't support them?
- What occurs when dependency updates introduce breaking API changes in external crates?
- How does the system handle references to "storage_locations" during the migration period to "storages"?
- What happens when kalamdb-commons types are updated - how do dependent crates handle version mismatches?
- How does the system handle circular dependencies if kalamdb-commons depends on other crates?
- What occurs when kalamdb-live loses connection to kalamdb-store or kalamdb-sql during active subscriptions?
- How are cached DataFusion expressions invalidated when query semantics change?
- What happens when a SQL function requires functionality not available in DataFusion's built-in UDFs?
- What occurs when documentation is moved during reorganization and external links break?
- How does the Docker container handle configuration file updates without rebuilding the image?
- What happens when persistent volumes in docker-compose contain data from incompatible schema versions?
- How does the Docker image behave when required environment variables are not provided?
- What occurs when a batch SQL request contains one valid and one invalid statement - are all results returned?
- How does the system handle WebSocket "last_rows" request when the table has fewer rows than requested?
- What happens when KILL LIVE QUERY is executed for a subscription that has already disconnected?
- How does DROP TABLE behave when active subscriptions exist but the subscription count is zero (race condition)?
- What occurs when system.live_queries is queried while subscriptions are being created/destroyed rapidly?
- How does the system handle job parameters that contain special characters or very long strings?
- What happens when a job completes but the trace information is unavailable (null case)?
- How does DESCRIBE TABLE display schema history when there are hundreds of schema versions?
- What occurs when SHOW TABLE STATS is executed for a table that has never been flushed?
- How does the system prevent subscription attempts on shared tables disguised through views or aliases?
- What happens when kalamdb-sql receives SQL that would violate stateless operation (e.g., session state mutation)?
- What occurs when INSERT INTO system.users is executed without providing required fields (user_id or username)?
- How does UPDATE system.users handle partial updates when some fields are not specified in SET clause?
- What happens when DELETE FROM system.users attempts to remove a user that has active data in user tables?
- How does the system handle UPDATE operations with malformed JSON in the metadata field?
- What occurs when INSERT attempts to add a user with empty string user_id or username?
- How does SELECT FROM system.users perform with thousands of users in the table?
- What happens when concurrent INSERT operations attempt to create the same user_id simultaneously?
- How does UPDATE handle setting metadata to NULL vs empty JSON object '{}'?

## Requirements *(mandatory)*

### Functional Requirements

#### Parametrized Query Support

- **FR-001**: System MUST accept SQL queries with positional parameter placeholders ($1, $2, etc.) via the `/api/sql` endpoint
- **FR-002**: API request body MUST support a format containing both `sql` (query string) and `params` (array of parameter values)
- **FR-003**: System MUST validate that the number of provided parameters matches the number of placeholders in the query
- **FR-004**: System MUST compile parametrized queries into reusable execution plans on first execution
- **FR-005**: System MUST cache compiled query execution plans indexed by the normalized query structure
- **FR-006**: System MUST substitute parameter values into cached execution plans without recompilation
- **FR-007**: System MUST support parameter types: string, integer, float, boolean, timestamp
- **FR-008**: System MUST return clear error messages when parameter types don't match expected column types
- **FR-009**: API response MUST optionally include query execution time when configured in `config.toml`

#### Automatic Flushing System

- **FR-010**: System MUST support configuration of automatic flush intervals per table at creation time
- **FR-011**: System MUST initialize a scheduler service that monitors all tables with flush configurations
- **FR-012**: Scheduler MUST trigger flush jobs at configured intervals for each table
- **FR-013**: Flush jobs MUST group buffered data by user_id before writing to storage
- **FR-014**: System MUST support configurable storage location path templates with variables: {storageLocation}, {namespace}, {userId}, {tableName}, {shard}
- **FR-015**: System MUST provide default storage location in `config.toml` (defaulting to `./data/storage`)
- **FR-016**: System MUST support separate path templates for user tables vs shared tables
- **FR-017**: User table default path template MUST be: `{storageLocation}/{namespace}/users/{userId}/{tableName}/`
- **FR-018**: Shared table default path template MUST be: `{storageLocation}/{namespace}/{tableName}/`
- **FR-019**: System MUST support configurable sharding strategies for distributing data across storage locations
- **FR-020**: System MUST provide a default alphabetic sharding strategy (a-z) when no custom strategy is specified
- **FR-021**: Flush jobs MUST write data in Parquet format to the determined storage locations
- **FR-022**: System MUST track flush job status using an actor model for observability
- **FR-023**: Each Parquet file MUST include metadata indicating the schema version used

#### Manual Flushing Commands

- **FR-024**: System MUST support SQL command: `FLUSH TABLE <namespace>.<table_name>`
- **FR-025**: System MUST support SQL command: `FLUSH ALL TABLES` to flush all tables with buffered data
- **FR-026**: Manual flush commands MUST be synchronous, returning only after flush operation completes
- **FR-027**: Flush command response MUST include number of records flushed and target storage location
- **FR-028**: System MUST automatically flush all tables during server shutdown sequence before process termination
- **FR-029**: System MUST prevent concurrent flush operations on the same table (queue subsequent requests)

#### Session-Level Table Caching

- **FR-030**: System MUST maintain a per-user session context for database operations
- **FR-031**: Session context MUST cache table registrations for tables accessed during the session
- **FR-032**: System MUST reuse cached table registrations for subsequent queries within the same session
- **FR-033**: System MUST implement a configurable timeout for cached table registrations (LRU or time-based eviction)
- **FR-034**: System MUST automatically evict unused table registrations based on eviction policy
- **FR-035**: System MUST detect schema changes and invalidate cached registrations when schema modifications occur
- **FR-036**: System MUST validate cached table registrations still reference existing tables before query execution

#### Namespace Validation

- **FR-037**: System MUST validate namespace existence before creating any user, shared, or stream table
- **FR-038**: System MUST return error "Namespace '<namespace>' does not exist" when table creation references non-existent namespace
- **FR-039**: Error message MUST include guidance: "Create it first with CREATE NAMESPACE."
- **FR-040**: Validation MUST be transactional to prevent race conditions between validation and table creation

#### Code Quality and Architectural Improvements

- **FR-041**: System MUST provide a common base implementation for system table providers to eliminate code duplication
- **FR-042**: System MUST define all system table names in a centralized location (single source of truth)
- **FR-043**: System MUST consistently use type-safe wrappers (NamespaceId, TableName, UserId) instead of raw strings throughout the codebase
- **FR-044**: All scan() functions MUST include documentation explaining their purpose, key parameter usage, and architectural role
- **FR-045**: Column family naming logic MUST be centralized in helper functions instead of inline string formatting
- **FR-046**: Validation logic for insert operations MUST be shared between user and shared table providers
- **FR-047**: System MUST store metadata columns ("_deleted", "_updated") efficiently without repeated string serialization
- **FR-048**: System table constant strings (like "SHOW BACKUP FOR DATABASE") MUST be defined once as enums or constants
- **FR-062**: All Rust crate dependencies MUST be updated to their latest compatible versions
- **FR-063**: README documentation MUST be rewritten to accurately reflect current architecture
- **FR-064**: README MUST minimize Parquet-specific details (mention once maximum)
- **FR-065**: README MUST document that WebSocket connections are direct to the server (no intermediary service)
- **FR-066**: DDL statement definitions and models MUST be located in kalamdb-sql crate where they logically belong
- **FR-067**: kalamdb-sql MUST access storage through kalamdb-store abstraction layer instead of direct RocksDB calls
- **FR-068**: System tables MUST use "system" as the default catalog name consistently
- **FR-069**: Test framework MUST support configuration to run tests against local server or temporary test server
- **FR-070**: System table "storage_locations" MUST be renamed to "storages" across all code, configuration, and documentation
- **FR-077**: A kalamdb-commons crate MUST be created to consolidate shared models, helpers, and types
- **FR-078**: kalamdb-commons MUST include type-safe models: UserId, NamespaceId, TableName, TableType
- **FR-079**: kalamdb-commons MUST include system table name constants (centralized enum or constants)
- **FR-080**: kalamdb-commons MUST include shared error types used across kalamdb-core, kalamdb-sql, and kalamdb-store
- **FR-081**: kalamdb-commons MUST include configuration models from kalamdb-server that other crates depend on
- **FR-082**: kalamdb-commons MUST include system helper functions used across multiple crates
- **FR-083**: Release build configuration MUST exclude testing and dev-only dependencies from final binary
- **FR-084**: Binary size MUST be audited to identify and remove unused dependencies
- **FR-085**: A kalamdb-live crate MUST be created to manage live query subscriptions separately from core logic
- **FR-086**: kalamdb-live MUST handle WebSocket subscription lifecycle and client notification
- **FR-087**: kalamdb-live MUST communicate with kalamdb-store for data access and kalamdb-sql for query execution
- **FR-088**: Live query expression evaluation MUST use DataFusion Expression objects
- **FR-089**: DataFusion Expression objects for live query filters MUST be compiled once and cached
- **FR-090**: SQL custom functions MUST leverage DataFusion's UDF (User Defined Function) infrastructure where applicable
- **FR-091**: SQL function implementations MUST reuse DataFusion built-in functions when functionality overlaps

#### Documentation Organization and Deployment

- **FR-092**: Documentation folder (/docs) MUST be organized into clear categorical subfolders
- **FR-093**: /docs/build/ MUST contain build instructions, compilation guides, and dependency information
- **FR-094**: /docs/quickstart/ MUST contain getting started guides, basic examples, and initial setup instructions
- **FR-095**: /docs/architecture/ MUST contain system design documents, architectural decisions, and component diagrams
- **FR-096**: Outdated and redundant documentation files MUST be identified and removed from /docs
- **FR-097**: All Docker-related files MUST be located in /docker folder at repository root
- **FR-098**: A production-ready Dockerfile MUST exist in /docker folder that builds KalamDB server
- **FR-099**: Dockerfile MUST use multi-stage builds to minimize final image size
- **FR-100**: Dockerfile MUST include only runtime dependencies in the final image (no build tools)
- **FR-101**: A docker-compose.yml MUST exist in /docker folder for orchestrating KalamDB deployment
- **FR-102**: docker-compose.yml MUST configure KalamDB server with appropriate environment variables
- **FR-103**: docker-compose.yml MUST define persistent volume mounts for data storage
- **FR-104**: docker-compose.yml MUST configure networking to expose appropriate ports (API, WebSocket)
- **FR-105**: Docker image MUST be configurable via environment variables (config.toml overrides)

#### Storage Backend Abstraction

- **FR-071**: System MUST define a storage backend trait/interface that abstracts storage operations
- **FR-072**: Storage trait MUST include operations for: get, put, delete, scan, batch operations, column family management
- **FR-073**: RocksDB implementation MUST implement the storage trait without exposing RocksDB-specific types
- **FR-074**: Storage abstraction MUST support pluggable backends (Sled, Redis, or custom implementations)
- **FR-075**: Column family concept MUST be abstracted to work with storage backends that don't natively support it
- **FR-076**: All existing RocksDB column family usage MUST be migrated to use the abstracted storage trait

#### Configuration and System Management

- **FR-049**: RocksDB storage directory MUST be configurable via `config.toml`
- **FR-050**: RocksDB storage directory MUST default to a location relative to the server binary (not temporary directory)
- **FR-051**: System MUST log RocksDB database size at server startup and periodically during operation
- **FR-052**: Server startup logs MUST include Git branch name and commit revision in version information
- **FR-053**: Configuration MUST support enabling/disabling query execution time reporting in API responses
- **FR-054**: System MUST support configurable localhost authentication bypass (allowing queries without JWT)
- **FR-055**: When localhost bypass is enabled, configuration MUST specify default user_id for localhost connections (defaulting to "system")
- **FR-056**: Non-localhost connections MUST always require valid JWT with user_id claim

#### Enhanced API Features and Live Query Improvements

- **FR-106**: System MUST accept multiple SQL statements separated by semicolons in a single `/api/sql` request
- **FR-107**: Multiple SQL statements MUST execute in sequence, with each statement's result returned separately
- **FR-108**: If any statement in a batch fails, subsequent statements MUST NOT execute and the error MUST be clearly indicated
- **FR-109**: WebSocket subscription options MUST support "last_rows" parameter to fetch initial data
- **FR-110**: When "last_rows": N is specified, system MUST immediately return the N most recent rows matching the subscription filter
- **FR-111**: Initial data fetch MUST complete before real-time change notifications begin
- **FR-112**: System MUST track active subscriptions per table to enable dependency checking
- **FR-113**: DROP TABLE command MUST fail if active live query subscriptions exist for that table
- **FR-114**: DROP TABLE error message MUST include the count of active subscriptions preventing the operation
- **FR-115**: System MUST support SQL command: `KILL LIVE QUERY <live_id>` to manually terminate subscriptions
- **FR-116**: KILL LIVE QUERY MUST disconnect the WebSocket subscription and remove it from system.live_queries
- **FR-117**: system.live_queries table MUST include an "options" column storing JSON-encoded subscription options
- **FR-118**: system.live_queries table MUST include a "changes" column tracking total notifications delivered
- **FR-119**: system.live_queries table MUST include a "node" column identifying which cluster node owns the WebSocket connection
- **FR-120**: system.jobs table MUST include a "parameters" column storing an array of job input parameters
- **FR-121**: system.jobs table MUST include a "result" column storing the job outcome as a string
- **FR-122**: system.jobs table MUST include a "trace" column storing execution context/location information
- **FR-123**: system.jobs table MUST include "memory_used" and "cpu_used" columns for resource tracking
- **FR-124**: DESCRIBE TABLE output MUST include current_schema_version field
- **FR-125**: DESCRIBE TABLE output MUST reference system.table_schemas for viewing schema history
- **FR-126**: System MUST support SQL command: `SHOW TABLE STATS <table_name>` returning row counts and storage metrics
- **FR-127**: SHOW TABLE STATS MUST display: buffered row count, flushed row count, total storage size, last flush timestamp
- **FR-128**: System MUST prevent subscription creation on shared tables to protect against performance issues
- **FR-129**: When shared table subscription is attempted, system MUST return error: "Live query subscriptions not supported on shared tables"
- **FR-130**: kalamdb-sql MUST be designed as stateless and idempotent to support future Raft consensus replication
- **FR-131**: kalamdb-sql architecture MUST support optional change event emission for future cluster replication

#### User Management SQL Commands

- **FR-132**: System MUST support standard SQL INSERT syntax for adding users: `INSERT INTO system.users (user_id, username, metadata) VALUES (...)`
- **FR-133**: System MUST support standard SQL UPDATE syntax for modifying users: `UPDATE system.users SET username = '...', metadata = '...' WHERE user_id = '...'`
- **FR-134**: System MUST support standard SQL DELETE syntax for removing users: `DELETE FROM system.users WHERE user_id = '...'`
- **FR-135**: System MUST validate user_id uniqueness on INSERT and return error "User with user_id 'X' already exists" for duplicates
- **FR-136**: System MUST validate user existence on UPDATE and return error "User with user_id 'X' not found" if user doesn't exist
- **FR-137**: System MUST validate user existence on DELETE and return error "User with user_id 'X' not found" if user doesn't exist
- **FR-138**: System MUST validate metadata field as valid JSON when provided in INSERT or UPDATE operations
- **FR-139**: System MUST automatically set created_at timestamp on INSERT using current server time
- **FR-140**: System MUST automatically update updated_at timestamp on UPDATE using current server time
- **FR-141**: System MUST support SELECT queries on system.users with filtering (WHERE), ordering (ORDER BY), and limiting (LIMIT)
- **FR-142**: System MUST support partial updates where only specified fields are modified (e.g., UPDATE only username without changing metadata)
- **FR-143**: username field MUST be required (NOT NULL) on INSERT operations
- **FR-144**: user_id field MUST be required (NOT NULL) on INSERT operations
- **FR-145**: metadata field MUST be optional (nullable) and default to NULL if not provided

#### Integration Testing Requirements

- **FR-146**: Each user story MUST have a dedicated integration test file following the naming convention test_{feature_name}.rs
- **FR-147**: Integration tests MUST use the common TestServer harness from backend/tests/integration/common/mod.rs
- **FR-148**: Integration tests MUST execute SQL commands via the /api/sql endpoint to test end-to-end functionality
- **FR-149**: Integration tests MUST verify both success cases and error cases with appropriate error messages
- **FR-150**: Integration tests MUST clean up test data and server resources after execution
- **FR-151**: Each acceptance scenario in a user story MUST have at least one corresponding integration test
- **FR-152**: Integration tests MUST be executable against both temporary test servers and local development servers
- **FR-153**: Integration tests MUST include performance validation where success criteria specify timing requirements
- **FR-154**: Integration tests MUST verify data persistence by querying after operations complete
- **FR-155**: Integration test documentation MUST reference the specific user story and acceptance scenarios being tested

#### Live Query Change Detection Testing

- **FR-156**: Integration tests MUST verify INSERT operation notifications are received by active WebSocket subscriptions
- **FR-157**: Integration tests MUST verify UPDATE operation notifications include both old and new values
- **FR-158**: Integration tests MUST verify DELETE operation notifications include deleted row data and _deleted flag
- **FR-159**: Integration tests MUST verify concurrent writers do not cause message loss or duplication in subscriptions
- **FR-160**: Integration tests MUST simulate realistic AI agent scenarios with human client subscriptions
- **FR-161**: Integration tests MUST verify notification ordering matches the chronological order of operations
- **FR-162**: Integration tests MUST validate the changes counter in system.live_queries accurately reflects delivered notifications
- **FR-163**: Integration tests MUST verify multiple concurrent subscriptions to the same table operate independently
- **FR-164**: Integration tests MUST test subscription reconnection scenarios without data loss
- **FR-165**: Integration tests MUST validate high-frequency change delivery (1000+ notifications) without errors

#### Memory Leak and Performance Stress Testing

- **FR-166**: Stress tests MUST monitor memory usage at regular intervals during sustained load
- **FR-167**: Stress tests MUST verify memory growth does not exceed 10% over baseline during extended operations
- **FR-168**: Stress tests MUST validate WebSocket connections remain stable under high load (no unexpected disconnections)
- **FR-169**: Stress tests MUST verify CPU usage remains reasonable (< 80% average) during sustained write operations
- **FR-170**: Stress tests MUST validate WebSocket connection cleanup (no connection leaks after subscription termination)
- **FR-171**: Stress tests MUST verify memory is properly released after stress operations complete
- **FR-172**: Stress tests MUST validate query performance remains acceptable (< 500ms p95) during concurrent load
- **FR-173**: Stress tests MUST verify flush operations during stress do not cause memory accumulation
- **FR-174**: Stress tests MUST monitor actor system health (no mailbox overflow or stuck actors)
- **FR-175**: Stress tests MUST validate system degrades gracefully (slower responses) rather than crashing under extreme load

#### Data Organization and Query Optimization

- **FR-057**: All scan operations on user-partitioned data MUST filter by user_id at the storage level
- **FR-058**: Scan operations on stream tables MUST filter by user_id to prevent full table scans
- **FR-059**: System MUST prevent users from subscribing to shared tables via WebSocket (performance protection)
- **FR-060**: When querying user tables with `.user.` qualifier (e.g., `FROM namespace1.user.user_files`), system MUST require X-USER-ID header
- **FR-061**: System MUST substitute `.user.` qualifier with actual user_id from X-USER-ID header in query resolution

### Key Entities

- **KalamLink**: Standalone Rust library crate providing all KalamDB connection, query execution, authentication, and subscription functionality (WebAssembly compatible)
- **KalamLinkClient**: Main client struct in `kalam-link` managing HTTP connections, authentication state, and WebSocket subscriptions
- **QueryExecutor**: Component in `kalam-link` responsible for sending SQL queries via REST API and parsing responses
- **SubscriptionManager**: Component in `kalam-link` managing WebSocket connections for live query subscriptions with event streaming
- **AuthProvider**: Component in `kalam-link` handling JWT/API key authentication and header injection
- **KalamCLI**: Interactive command-line interface binary that uses `kalam-link` for all database operations
- **CLISession**: CLI session state managing user configuration, connection, and active subscriptions (via `kalam-link`)
- **CLIConfiguration**: User configuration stored in `~/.kalam/config.toml` with connection defaults and output preferences (no tenant)
- **OutputFormatter**: CLI component rendering query results in different formats (table, JSON, CSV) - does NOT exist in `kalam-link`
- **CommandParser**: CLI component parsing user input into SQL queries or backslash commands
- **AutoCompleter**: CLI component providing TAB completion for SQL keywords (SELECT, INSERT, CREATE, etc.)
- **CommandHistory**: Persistent storage of user-entered commands accessible via arrow keys across CLI sessions
- **ParametrizedQuery**: Represents a SQL query with positional parameter placeholders and the array of parameter values to be substituted
- **QueryExecutionPlan**: A compiled and optimized execution plan for a specific query structure, cached for reuse with different parameter values
- **FlushJob**: Represents a scheduled or manual operation to persist buffered table data to Parquet files in storage locations
- **FlushConfiguration**: Defines automatic flush behavior for a table including interval, storage path template, and sharding strategy
- **StorageLocation**: A configured destination for persisted data with path template and template variables (namespace, userId, shard, tableName)
- **ShardingStrategy**: A function or algorithm that determines which shard a particular data subset should be written to
- **SessionCache**: A per-user session context maintaining cached table registrations and other session-specific state
- **TableRegistration**: A cached reference to a table's schema and metadata within a session, enabling fast query execution without re-registration
- **SystemTableProvider**: Base abstraction for system tables with common scanning, filtering, and projection logic
- **StorageBackend**: Trait/interface defining storage operations (get, put, delete, scan, batch) that can be implemented by different storage engines
- **StorageTrait**: Generic storage abstraction supporting pluggable backends (RocksDB, Sled, Redis, etc.) with consistent APIs
- **kalamdb-commons**: Shared crate containing type-safe models, constants, error types, and helper functions used across all other crates
- **LiveQuerySubscription**: Represents an active WebSocket subscription to a query with filter expressions and client notification state
- **CachedExpression**: A compiled and cached DataFusion Expression object used for efficient live query filtering without repeated parsing
- **DocumentationCategory**: Logical grouping of documentation files (build, quickstart, architecture) for organized navigation
- **DockerImage**: Containerized KalamDB server with runtime dependencies, built via multi-stage Dockerfile
- **DockerCompose**: Orchestration configuration defining services, volumes, networks, and environment for KalamDB deployment
- **BatchSQLRequest**: API request containing multiple semicolon-separated SQL statements to be executed sequentially
- **BatchSQLResponse**: API response containing ordered results for each statement in a batch execution
- **SubscriptionOptions**: Configuration for WebSocket subscriptions including "last_rows" for initial data fetch
- **ActiveSubscriptionTracker**: Component tracking live query subscriptions per table to enable dependency checking for DDL operations
- **EnhancedSystemTable**: Updated system tables (live_queries, jobs) with additional columns for observability and cluster awareness
- **SchemaHistory**: Queryable record of all schema versions for a table accessible through system.table_schemas
- **TableStatistics**: Metrics about a table including row counts, storage size, and buffer status
- **StatelessSQLEngine**: Design pattern for kalamdb-sql ensuring operations are idempotent and replayable for future cluster replication
- **UserManagementCommand**: SQL command (INSERT/UPDATE/DELETE) for managing user records in system.users table
- **UserRecord**: Data structure representing a user in system.users with user_id, username, metadata, created_at, and updated_at fields
- **IntegrationTestSuite**: Comprehensive test suite organized by user story, using TestServer harness and executing via /api/sql endpoint
- **TestServer**: Common test harness providing server lifecycle management, SQL execution, and cleanup utilities for integration testing

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-CLI-001**: CLI establishes connection to KalamDB server (via `kalam-link`) and displays prompt within 2 seconds
- **SC-CLI-002**: SQL query results for simple SELECT statements (< 1000 rows) are displayed within 500ms including network, `kalam-link` processing, and formatting time
- **SC-CLI-003**: CLI successfully formats query results as ASCII tables with aligned columns for result sets up to 10,000 rows
- **SC-CLI-004**: WebSocket subscriptions (via `kalam-link`) are established within 1 second and display first live update within 100ms of server event
- **SC-CLI-005**: CLI handles at least 1000 live updates per second without dropping messages or UI freezing
- **SC-CLI-006**: Command history stores and retrieves at least 1000 previous commands across sessions
- **SC-CLI-007**: Batch file execution processes at least 100 SQL queries from a file within 30 seconds
- **SC-CLI-008**: CLI binary size is under 20MB (release build, stripped)
- **SC-CLI-009**: Memory usage remains stable under 50MB during interactive sessions with subscriptions
- **SC-CLI-010**: CLI successfully reconnects (via `kalam-link`) after network interruption with clear user notification
- **SC-CLI-011**: `kalam-link` crate compiles successfully to wasm32-unknown-unknown target without errors
- **SC-CLI-012**: `kalam-link` can be used independently (without CLI) to execute queries and subscriptions programmatically
- **SC-CLI-013**: Auto-completion suggestions appear within 50ms of TAB key press
- **SC-CLI-014**: Auto-completion correctly suggests all supported SQL keywords (at least 20 keywords)
- **SC-CLI-015**: CLI project is located in `/cli` folder at repository root, completely separate from `/backend`
- **SC-001**: Parametrized queries with cached execution plans execute at least 40% faster than non-parametrized equivalent queries with identical logic
- **SC-002**: Session-level table registration caching reduces query execution time for repeated table access by at least 30% compared to per-query registration
- **SC-003**: Automatic flush jobs complete successfully for tables with up to 1 million buffered records within 5 minutes
- **SC-004**: Flush operations organize data correctly with 100% accuracy (no misplaced files, correct user/namespace/shard paths)
- **SC-005**: Manual flush commands complete synchronously and return status within 10 seconds for tables with under 100,000 records
- **SC-006**: Namespace validation prevents 100% of table creation attempts in non-existent namespaces
- **SC-007**: System handles at least 100 concurrent parametrized queries without execution plan cache contention or errors
- **SC-008**: Query plan cache reduces DataFusion compilation calls by at least 80% for workloads with repeated query patterns
- **SC-009**: Flush job scheduler maintains configured intervals with less than 5% drift over 24-hour periods
- **SC-010**: Server shutdown with automatic flush completes within 30 seconds for databases with under 10 active tables
- **SC-011**: Session cache eviction policy maintains memory usage under configured limits while maximizing hit rate
- **SC-012**: Code duplication in system table providers reduces by at least 70% after refactoring to shared base implementation
- **SC-013**: All public scan() functions have comprehensive documentation with usage examples
- **SC-014**: Type-safe wrappers (NamespaceId, TableName, UserId) replace at least 95% of raw string usage for identifiers
- **SC-015**: All Rust dependencies are updated to latest compatible versions without breaking changes
- **SC-016**: README accurately documents current architecture with WebSocket information and minimal Parquet references
- **SC-017**: All DDL definitions are consolidated in kalamdb-sql crate (100% migration)
- **SC-018**: kalamdb-sql eliminates all direct RocksDB calls and uses kalamdb-store abstraction (100% migration)
- **SC-019**: Storage backend abstraction trait enables at least one alternative backend implementation (proof of concept)
- **SC-020**: System table "storage_locations" is fully renamed to "storages" with no legacy references remaining
- **SC-021**: Test framework successfully runs all tests against both local and temporary server configurations
- **SC-022**: kalamdb-commons crate consolidates at least 95% of shared types and constants across other crates
- **SC-023**: Release binary size reduces by at least 10% after removing test-only dependencies and unused crates
- **SC-024**: kalamdb-live crate successfully manages all live query subscriptions with clear separation from core logic
- **SC-025**: Live query filter evaluation with cached DataFusion expressions executes at least 50% faster than string-based parsing
- **SC-026**: SQL custom functions leverage DataFusion UDFs for at least 80% of common function operations
- **SC-027**: Documentation in /docs is organized into 3 clear categories with no files in root folder
- **SC-028**: Docker image builds successfully and starts KalamDB server within 30 seconds
- **SC-029**: docker-compose brings up fully functional KalamDB system with single command
- **SC-030**: Docker image size is under 100MB (excluding data volumes)
- **SC-031**: Batch SQL execution completes all statements successfully with correct result ordering 100% of the time
- **SC-032**: WebSocket subscriptions with "last_rows" fetch complete initial data within 500ms for N ≤ 1000
- **SC-033**: DROP TABLE dependency checking correctly identifies and prevents drops with active subscriptions 100% of the time
- **SC-034**: KILL LIVE QUERY command terminates subscriptions within 2 seconds
- **SC-035**: Enhanced system.live_queries columns (options, changes, node) are populated accurately for all subscriptions
- **SC-036**: Enhanced system.jobs columns (parameters, result, trace, memory_used, cpu_used) capture data for at least 95% of jobs
- **SC-037**: DESCRIBE TABLE with schema history returns results within 100ms
- **SC-038**: SHOW TABLE STATS executes and returns accurate metrics within 50ms
- **SC-039**: Shared table subscription prevention blocks 100% of attempts with clear error messages
- **SC-040**: kalamdb-sql stateless design enables identical query results across cluster nodes (verifiable through testing)
- **SC-041**: User INSERT operations complete within 10ms and are immediately queryable
- **SC-042**: User UPDATE operations modify only specified fields and complete within 10ms
- **SC-043**: User DELETE operations remove users successfully and return appropriate errors for non-existent users
- **SC-044**: User uniqueness validation prevents duplicate user_id with clear error messages 100% of the time
- **SC-045**: JSON metadata validation rejects invalid JSON with clear error messages 100% of the time
- **SC-046**: Timestamp fields (created_at, updated_at) are automatically managed with accurate server time
- **SC-047**: Each user story has a complete integration test file with all acceptance scenarios covered
- **SC-048**: Integration tests achieve at least 90% code coverage for new functionality
- **SC-049**: All integration tests pass consistently on both Windows and Linux platforms
- **SC-050**: Integration tests execute within reasonable time limits (full suite under 5 minutes)
- **SC-051**: Live query subscriptions deliver 100% of INSERT notifications without loss or duplication
- **SC-052**: Live query UPDATE notifications include both old and new values in 100% of cases
- **SC-053**: Live query DELETE notifications correctly identify soft-deleted rows with _deleted flag
- **SC-054**: Concurrent writers with live query listeners maintain ordering and deliver all changes within 50ms
- **SC-055**: system.live_queries changes counter matches actual delivered notifications with 100% accuracy
- **SC-056**: Memory usage during stress test (10 writers, 20 listeners, 5 minutes) grows less than 10% over baseline
- **SC-057**: WebSocket connections under stress test (100,000+ notifications) maintain 99.9% uptime without unexpected disconnections
- **SC-058**: Query performance during stress test maintains p95 response time under 500ms
- **SC-059**: Memory is fully released (within 5% of baseline) within 60 seconds after stress test completion
- **SC-060**: System under extreme load degrades gracefully with slower responses rather than crashes or errors

### Documentation Success Criteria (Constitution Principle VIII)

- **SC-DOC-001**: All public APIs have comprehensive rustdoc comments with real-world examples
- **SC-DOC-002**: Module-level documentation explains purpose and architectural role
- **SC-DOC-003**: Complex algorithms and architectural patterns have inline comments explaining rationale
- **SC-DOC-004**: Architecture Decision Records (ADRs) document key design choices for query caching and flush architecture
- **SC-DOC-005**: Code review verification confirms documentation requirements are met

## Assumptions

1. **CLI Project Location**: The `/cli` folder at repository root is the designated location for all client tooling (not inside `/backend`)
2. **kalam-link WebAssembly Compatibility**: The `kalam-link` crate will be designed from the start with WebAssembly compilation as a target requirement
3. **No Multi-Tenancy**: KalamDB does not require tenant isolation; all operations are scoped by user_id only (no tenant_id)
4. **CLI Platform Support**: The CLI will be built for macOS, Linux, and Windows platforms with standard terminal emulators
5. **Terminal Capabilities**: Users have ANSI-compatible terminal emulators supporting basic cursor movement and color codes
6. **Rust Ecosystem**: Established Rust crates (tokio, reqwest, tungstenite, crossterm, rustyline, tabled) are mature and suitable for CLI development
7. **Single User Session**: Initial CLI implementation supports one user session per CLI instance (no multi-user switching)
8. **File System Access**: Users have read/write permissions to their home directory for config and history files
9. **Network Requirements**: CLI users have direct HTTP/WebSocket network access to KalamDB server (no proxy considerations in initial version)
10. **Auto-completion Scope**: Basic SQL keyword completion is sufficient; table name and column name completion are future enhancements
11. **DataFusion Integration**: The project uses Apache DataFusion for query compilation and execution, and its query plan caching capabilities are available
2. **Storage Format**: Parquet is the established format for persisted data; flush operations continue using this format
3. **RocksDB Usage**: The system currently uses RocksDB for buffering data before flush; this remains the buffer storage mechanism
4. **Authentication**: JWT-based authentication is already implemented; localhost bypass is an additional configuration option
5. **WebSocket Infrastructure**: WebSocket support for subscriptions exists; preventing shared table subscriptions is a policy enforcement addition
6. **Actor Model**: The project already uses actor patterns for some subsystems (like live queries); flush jobs follow the same pattern
7. **Configuration Format**: TOML format is the established configuration mechanism; new settings follow existing conventions
8. **Multi-tenancy Model**: User isolation and user_id-based data organization are core architectural principles already in place
9. **Namespace Concept**: Namespaces are an existing organizational structure; the change is enforcing their existence before table creation
10. **Default Sharding**: Alphabetic (a-z) sharding provides 26 shards initially; custom sharding functions can be added later
11. **Schema Versioning**: The ability to store metadata in Parquet files exists; schema version is one additional metadata field
12. **Session Context**: The concept of user sessions exists; table registration caching extends existing session infrastructure
13. **Backward Compatibility**: Changes are additive; existing non-parametrized queries continue working unchanged
14. **Performance Baseline**: Current system performance is measured and known, enabling validation of improvement targets
15. **Docker Availability**: Docker and docker-compose are available in the development and deployment environments
16. **Documentation Format**: Markdown is the standard format for documentation; reorganization maintains this format
17. **Existing Documentation**: Some documentation exists in /docs; reorganization improves structure rather than creating from scratch

## Out of Scope

The following items are explicitly NOT included in this feature specification:

1. **Multi-Tenancy Support**: Tenant isolation and tenant_id scoping are not included; all operations are user-scoped only
2. **Advanced Auto-completion**: Table name, column name, and context-aware SQL completion are future enhancements; only keyword completion is included
3. **kalam-link Full SDK Features**: Initial `kalam-link` focuses on core connectivity; advanced features like connection pooling, query result caching, and offline mode are future enhancements
4. **Browser WebAssembly Client**: While `kalam-link` is designed for WebAssembly compatibility, the actual browser client and JavaScript bindings are separate projects
5. **Visual Query Builder**: GUI-based query construction is a future CLI enhancement
6. **Multi-Subscription Management**: CLI limits to one active subscription at a time initially; concurrent subscription UI is a future enhancement
7. **Distributed Query Execution**: This feature focuses on single-node optimization; distributed query planning across Raft cluster nodes is separate
8. **Advanced Sharding Strategies**: Only alphabetic sharding is included; sophisticated sharding functions (consistent hashing, range-based, custom user functions) are future enhancements
9. **Query Result Caching**: This feature caches execution plans only; caching actual query results is a separate performance optimization
10. **Automatic Schema Migration**: Session cache invalidation detects schema changes but does not automatically migrate data; migration remains a separate concern
11. **Compaction Jobs**: Merging multiple Parquet files in storage is mentioned in notes but is a separate background maintenance feature
12. **User File Storage**: The `user_files` table concept mentioned in notes is a distinct feature, not part of this specification
13. **Workflow Triggers (KFlows)**: Event-driven workflows listening to streams are a separate feature area
14. **Raft Replication Logic**: While flush jobs must work in a Raft environment, the replication protocol itself is not part of this feature
15. **Client SDK Development**: TypeScript SDK and Python SDK are separate client-side projects (only `kalam-link` Rust library is included)
16. **Index Support**: Column-level indexes (BLOOM, SORTED) mentioned in notes are a separate query optimization feature
17. **Auto-increment Columns**: Automatic ID generation is a separate DDL enhancement
18. **Example Projects**: TypeScript TODO app example is separate documentation/sample code
19. **Binary Distribution**: Auto-deploy to GitHub releases is CI/CD pipeline work, not feature development
20. **Kubernetes/Helm Charts**: Container orchestration beyond docker-compose is separate infrastructure work

## Dependencies

- **kalam-link Crate**: Self-contained library with no dependency on KalamDB internals; communicates via public REST/WebSocket APIs only
- **REST API Endpoint**: `kalam-link` requires `/v1/api/sql` endpoint to be functional for query execution
- **WebSocket Endpoint**: `kalam-link` requires `/v1/ws` endpoint to be functional for live subscriptions
- **JWT Authentication**: `kalam-link` depends on server JWT validation for secure connections
- **Health Endpoint**: `kalam-link` depends on `/v1/health` endpoint for connection validation
- **kalam-cli Dependency**: CLI binary depends on `kalam-link` crate for all database communication (no direct HTTP/WebSocket code)
- **Existing Configuration System**: Requires `config.toml` parsing and validation infrastructure
- **DataFusion Query Engine**: Depends on DataFusion APIs for query compilation, execution plans, and parameter binding
- **RocksDB Storage Layer**: Requires RocksDB column families for buffering user and shared table data
- **Parquet Writing Infrastructure**: Depends on existing Parquet serialization and file writing capabilities
- **Actor Framework**: Flush jobs depend on actor model infrastructure for job tracking and observability
- **Session Management**: Table registration caching depends on user session context tracking
- **Namespace Management**: Namespace validation requires functional namespace creation and metadata storage
- **Authentication System**: JWT validation and user_id extraction must be functional for secure parametrized queries
- **WebSocket Subscription System**: Preventing shared table subscriptions requires access to subscription logic

## Risks and Mitigations

### Risk: kalam-link WebAssembly Compatibility
**Impact**: Dependencies or features in `kalam-link` may not be compatible with WebAssembly compilation target  
**Mitigation**: Design `kalam-link` with wasm32-unknown-unknown target from day one; avoid OS-specific dependencies; use wasm-compatible async runtime (tokio with wasm feature); test WebAssembly compilation in CI/CD; use conditional compilation for platform-specific features

### Risk: CLI and kalam-link Architecture Coupling
**Impact**: Tight coupling between CLI and `kalam-link` could make the library difficult to use independently in other contexts  
**Mitigation**: Define clear API boundaries; `kalam-link` provides callback/stream interfaces for events; CLI consumes but doesn't dictate `kalam-link` design; document `kalam-link` API independently; create integration tests using `kalam-link` without CLI

### Risk: Auto-completion Performance
**Impact**: TAB completion may cause UI lag if keyword matching is not optimized  
**Mitigation**: Use pre-built trie or hash-based keyword lookup; limit completion suggestions to reasonable count (e.g., 20); implement async completion to avoid blocking UI; profile completion performance

### Risk: CLI WebSocket Connection Stability
**Impact**: Unstable WebSocket connections could cause subscription interruptions and data loss in live query streaming  
**Mitigation**: Implement automatic reconnection with exponential backoff; buffer missed events during disconnection; display clear connection status to user; implement heartbeat/ping-pong to detect stale connections early

### Risk: Terminal Compatibility Issues
**Impact**: CLI rendering may break on different terminal emulators or operating systems  
**Mitigation**: Use well-tested terminal libraries (crossterm/ratatui); test on major platforms (macOS, Linux, Windows); provide fallback plain-text mode; document terminal requirements

### Risk: Large Result Set Performance
**Impact**: Displaying very large query results (100k+ rows) in the terminal could freeze the UI or consume excessive memory  
**Mitigation**: Implement pagination with automatic "Press Enter for more" prompts; limit default display to 1000 rows with option to show more; stream results instead of buffering all in memory; add `LIMIT` suggestion in help text

### Risk: Config File Security
**Impact**: Storing JWT tokens in plain-text config file at `~/.kalam/config.toml` exposes credentials  
**Mitigation**: Set restrictive file permissions (0600) on config file; support environment variables for sensitive values (KALAM_TOKEN); add warning about token storage in documentation; consider OS keychain integration in future

### Risk: Command History Injection
**Impact**: Malicious commands stored in history could execute unintended operations if replayed  
**Mitigation**: Sanitize history file on read; warn before executing potentially destructive commands (DROP, DELETE) even from history; allow `\history clear` command

### Risk: Concurrent Subscription Management
**Impact**: Managing multiple concurrent subscriptions in a single CLI session could cause UI corruption or event mixing  
**Mitigation**: For initial implementation, limit to one active subscription at a time; display clear "subscription active" indicator; properly clean up subscription state on cancel

### Risk: Cross-Platform Build Complexity
**Impact**: Building CLI for multiple platforms (macOS, Linux, Windows) with different terminal capabilities increases maintenance burden  
**Mitigation**: Use cross-compilation in CI/CD; automate release builds for all platforms; test on each platform; use platform-agnostic libraries where possible

### Risk: Query Plan Cache Memory Growth
**Impact**: Unbounded query plan cache could exhaust server memory with diverse query patterns  
**Mitigation**: Implement LRU eviction policy with configurable cache size limits; monitor cache hit rates and memory usage

### Risk: Flush Job Backlog During High Write Volume
**Impact**: Flush jobs may fall behind during sustained high insert rates, causing buffer growth  
**Mitigation**: Implement flush job queuing with priority handling; add monitoring alerts for flush lag; support multiple concurrent flush workers

### Risk: Race Conditions in Table Registration Cache
**Impact**: Concurrent queries may cause duplicate table registrations or cache inconsistency  
**Mitigation**: Use proper locking or lock-free data structures for session cache access; validate cache consistency in tests

### Risk: Storage Path Injection Vulnerabilities
**Impact**: Malicious template variable values could cause writes outside intended directories  
**Mitigation**: Validate and sanitize all path template variables; use safe path joining functions; restrict allowed characters

### Risk: Flush Failure Data Loss
**Impact**: If flush operation fails after clearing buffer, data could be permanently lost  
**Mitigation**: Implement write-ahead logging for flush operations; only clear buffer after successful Parquet file write; support flush retry logic

### Risk: Schema Version Mismatch on Read
**Impact**: Reading Parquet files with incompatible schema versions could cause query failures  
**Mitigation**: Store schema version in file metadata; validate version compatibility on file open; support schema evolution rules

### Risk: Performance Regression from Validation Overhead
**Impact**: Adding namespace validation to table creation could slow down bulk table creation operations  
**Mitigation**: Cache namespace existence checks; use efficient lookup data structures; batch validation when possible

### Risk: Cache Invalidation Complexity
**Impact**: Detecting schema changes for cache invalidation may miss edge cases, causing stale cache usage  
**Mitigation**: Use schema version tracking; implement conservative invalidation (invalidate on any DDL); add manual cache clearing commands for troubleshooting

### Risk: Dependency Update Breaking Changes
**Impact**: Updating all dependencies to latest versions may introduce breaking API changes or incompatibilities  
**Mitigation**: Update dependencies incrementally with comprehensive test runs; pin major versions; maintain compatibility layer for critical dependencies; use semantic versioning strictly

### Risk: Storage Abstraction Performance Overhead
**Impact**: Adding an abstraction layer over RocksDB could introduce performance degradation from indirect calls  
**Mitigation**: Use zero-cost abstractions (traits with monomorphization); benchmark before/after abstraction; use inline hints for hot paths; consider trait objects only where dynamic dispatch is necessary

### Risk: Incomplete Storage Backend Migration
**Impact**: Missing direct RocksDB calls in kalamdb-sql could cause runtime errors or data inconsistencies  
**Mitigation**: Comprehensive code search for RocksDB imports; static analysis to detect direct usage; integration tests covering all storage operations; gradual migration with feature flags

### Risk: README Staleness After Update
**Impact**: Updated README may become outdated again as system evolves  
**Mitigation**: Include README review in pull request checklist; automate validation of code examples in documentation; link README sections to specific code locations with automated checks

### Risk: Storage Backend Feature Parity
**Impact**: Alternative storage backends (Sled, Redis) may lack features available in RocksDB (column families, transactions)  
**Mitigation**: Define minimum feature set for storage trait; document backend-specific capabilities; graceful degradation for optional features; clear error messages for unsupported operations

### Risk: kalamdb-commons Circular Dependencies
**Impact**: Creating a shared commons crate could introduce circular dependencies between crates  
**Mitigation**: Design commons crate as dependency-free foundation; only include pure data types and helpers; no logic that depends on other kalamdb crates; strict layered architecture

### Risk: Expression Cache Stale Data
**Impact**: Cached DataFusion expressions for live queries may not reflect updated query semantics or schema changes  
**Mitigation**: Include schema version in cache keys; invalidate cache on any schema DDL; implement cache TTL; add manual cache clear for troubleshooting

### Risk: kalamdb-live Communication Failures
**Impact**: If kalamdb-live loses connection to kalamdb-store or kalamdb-sql, subscriptions fail without graceful handling  
**Mitigation**: Implement connection retry logic; queue operations during temporary disconnections; notify clients of subscription errors; include health checks

### Risk: Binary Size Regression
**Impact**: Future dependency additions could reintroduce bloat after optimization efforts  
**Mitigation**: Add binary size checks to CI/CD pipeline; document approved dependency list; require justification for new dependencies; use feature flags to make dependencies optional

### Risk: DataFusion UDF Limitations
**Impact**: Some custom SQL functions may require features not available in DataFusion's UDF infrastructure  
**Mitigation**: Document DataFusion limitations; provide extension points for custom implementations; use DataFusion UDFs as default with fallback to custom code; contribute missing features upstream

### Risk: Documentation Link Breakage
**Impact**: Reorganizing /docs folder structure may break external links and bookmarks to documentation  
**Mitigation**: Create redirects or index file mapping old paths to new paths; communicate changes in release notes; use relative links within documentation; validate internal links after reorganization

### Risk: Docker Image Size Bloat
**Impact**: Including unnecessary build dependencies or layers could result in large Docker images  
**Mitigation**: Use multi-stage builds with separate builder and runtime stages; use Alpine or distroless base images; only COPY required artifacts; use .dockerignore to exclude unnecessary files; regularly audit image layers

### Risk: Docker Configuration Drift
**Impact**: docker-compose.yml configuration may diverge from actual deployment requirements  
**Mitigation**: Test docker-compose deployment in CI/CD; document required environment variables; provide example .env file; validate configuration matches production needs; include healthchecks

### Risk: Volume Permission Issues
**Impact**: Docker containers may have permission conflicts with host-mounted volumes for data persistence  
**Mitigation**: Document volume ownership requirements; use appropriate USER directive in Dockerfile; provide initialization scripts for volume setup; include troubleshooting guide for permission errors
