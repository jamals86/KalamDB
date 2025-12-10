# Feature Specification: Admin UI with Token-Based Authentication

**Feature Branch**: `015-admin-ui`  
**Created**: December 2, 2025  
**Status**: Draft  
**Input**: User description: "Create admin UI with token-based authentication, React dashboard for managing users/storages/namespaces, SQL studio with autocomplete, and storage browser"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Token-Based Authentication Login (Priority: P1)

As an administrator, I want to log in once with my username and password and receive an access token that I can use for all subsequent requests, so that I don't have to repeatedly send credentials with every API call.

**Why this priority**: Authentication is the foundation for all other features. Without secure token-based auth, no other admin functionality can be safely accessed. This is the critical security layer.

**Independent Test**: Can be fully tested by logging in through the UI, receiving a token, and using that token to make authenticated API calls. Delivers secure, stateless authentication.

**Acceptance Scenarios**:

1. **Given** I am on the login page, **When** I enter valid username and password and click login, **Then** I receive an access token and am redirected to the dashboard
2. **Given** I have a valid access token, **When** I make an API request with the token in the Authorization header, **Then** the request is authenticated successfully
3. **Given** I have an expired access token, **When** I make an API request, **Then** I receive an unauthorized error and am prompted to log in again
4. **Given** I am logged in, **When** I click logout, **Then** my session is invalidated and I am redirected to the login page

---

### User Story 2 - SQL Studio with Query Execution (Priority: P2)

As a database administrator, I want to write and execute SQL queries against the database with syntax highlighting and autocomplete, so that I can efficiently explore and manage data.

**Why this priority**: SQL query execution is the primary way administrators interact with the database. This enables data exploration, debugging, and ad-hoc operations.

**Independent Test**: Can be fully tested by opening SQL Studio, typing a query with autocomplete suggestions, executing it, and viewing results.

**Acceptance Scenarios**:

1. **Given** I am in SQL Studio, **When** I type a SQL query, **Then** I see syntax highlighting for SQL keywords, tables, and columns
2. **Given** I am typing a query, **When** I type a table name prefix, **Then** I see autocomplete suggestions for matching table names
3. **Given** I am typing a query after a table name, **When** I type a column prefix, **Then** I see autocomplete suggestions for columns in that table
4. **Given** I have written a valid query, **When** I click execute or press the keyboard shortcut, **Then** the query runs and results display in a data grid
5. **Given** I have executed a query with results, **When** I view the results, **Then** I can sort, filter, and paginate through the data
6. **Given** I execute an invalid query, **When** the query fails, **Then** I see a clear error message indicating the problem

---

### User Story 3 - User Management (Priority: P2)

As a system administrator, I want to view, create, edit, and delete users through the admin UI, so that I can manage who has access to the database.

**Why this priority**: User management is essential for access control and is a core administrative function that directly impacts security.

**Independent Test**: Can be fully tested by navigating to Users section, creating a new user, editing their role, and deleting a test user.

**Acceptance Scenarios**:

1. **Given** I am on the Users page, **When** the page loads, **Then** I see a list of all users with their username, role, and status
2. **Given** I am viewing the user list, **When** I click "Create User", **Then** I see a form to enter username, password, and role
3. **Given** I am creating a user, **When** I submit valid user details, **Then** the user is created and appears in the list
4. **Given** I am viewing a user, **When** I click edit, **Then** I can modify the user's role and save changes
5. **Given** I am viewing a user, **When** I click delete and confirm, **Then** the user is soft-deleted and no longer appears in the active list

---

### User Story 4 - Storage Management (Priority: P3)

As a system administrator, I want to view and manage storage configurations, so that I can control where data is persisted.

**Why this priority**: Storage management is important for infrastructure configuration but is typically a less frequent operation than user or data management.

**Independent Test**: Can be fully tested by navigating to Storages section, viewing storage details, and creating a new storage configuration.

**Acceptance Scenarios**:

1. **Given** I am on the Storages page, **When** the page loads, **Then** I see a list of all configured storages with their type, path, and status
2. **Given** I am viewing storages, **When** I click on a storage, **Then** I see detailed configuration and usage statistics
3. **Given** I am on the storage details page, **When** I browse folders, **Then** I can navigate the directory structure and see file listings

---

### User Story 5 - Namespace Management (Priority: P3)

As a database administrator, I want to view and manage namespaces, so that I can organize tables and control data isolation.

**Why this priority**: Namespaces provide logical data organization. While important, most installations use a limited number of namespaces set up initially.

**Independent Test**: Can be fully tested by viewing namespaces, creating a new namespace, and viewing tables within it.

**Acceptance Scenarios**:

1. **Given** I am on the Namespaces page, **When** the page loads, **Then** I see a list of all namespaces with table counts
2. **Given** I am viewing namespaces, **When** I click on a namespace, **Then** I see the tables and configuration for that namespace
3. **Given** I am viewing a namespace, **When** I click "Create Namespace", **Then** I can create a new namespace with a name

---

### User Story 6 - Storage Browser (Priority: P3)

As a database administrator, I want to browse files and folders within storages, so that I can inspect data files, segments, and understand storage usage.

**Why this priority**: Storage browsing is a diagnostic tool useful for troubleshooting and understanding data layout, but not essential for daily operations.

**Independent Test**: Can be fully tested by selecting a storage, navigating through folders, and viewing file metadata.

**Acceptance Scenarios**:

1. **Given** I am in the storage browser, **When** I select a storage, **Then** I see the root folder contents
2. **Given** I am viewing a folder, **When** I click on a subfolder, **Then** I navigate into that folder and see its contents
3. **Given** I am viewing folder contents, **When** I see files, **Then** I can view file metadata (name, size, modified date)
4. **Given** I am in a nested folder, **When** I click the breadcrumb navigation, **Then** I can navigate back to parent folders

---

### User Story 7 - Settings View (Priority: P4)

As a system administrator, I want to view database configuration settings, so that I can understand and verify the current system configuration.

**Why this priority**: Settings viewing is primarily informational and less interactive than other management functions.

**Independent Test**: Can be fully tested by navigating to Settings and viewing current configuration values.

**Acceptance Scenarios**:

1. **Given** I am on the Settings page, **When** the page loads, **Then** I see current database configuration settings organized by category
2. **Given** I am viewing settings, **When** I look at a setting, **Then** I see the setting name, current value, and description

---

### Edge Cases

- What happens when the access token expires while the user is actively using the UI?
  - UI silently refreshes token before expiration; redirects to login only if refresh fails
- What happens when a user tries to delete themselves?
  - System should prevent self-deletion with a clear error message
- What happens when SQL query execution takes too long?
  - System enforces 30-second timeout with visible countdown; user can cancel at any time
- How does the system handle concurrent admin sessions?
  - Multiple sessions are allowed; each session has its own token
- What happens when storage becomes unreachable while browsing?
  - Display error message and offer retry option

## Requirements *(mandatory)*

### Functional Requirements

**Authentication & Authorization**:
- **FR-001**: System MUST provide a login endpoint that accepts username/password and returns an access token
- **FR-002**: System MUST validate access tokens on all protected API endpoints
- **FR-003**: Access tokens MUST expire after a configurable time period (default: 24 hours)
- **FR-004**: System MUST provide a logout endpoint that invalidates the current session
- **FR-005**: System MUST return appropriate error messages for invalid or expired tokens
- **FR-006**: System MUST restrict Admin UI access to users with dba or system roles only
- **FR-007**: System MUST provide a token refresh endpoint that issues a new access token before expiration
- **FR-008**: UI MUST silently refresh tokens before expiration; redirect to login only when refresh fails
- **FR-009**: System MUST store access tokens in HttpOnly cookies (not accessible to JavaScript) for XSS protection

**Admin UI - General**:
- **FR-010**: System MUST serve a React-based single-page application at the `/ui` route
- **FR-011**: UI MUST use shadcn/ui component library for consistent styling
- **FR-012**: UI MUST persist authentication state across page refreshes via HttpOnly cookie
- **FR-013**: UI MUST provide navigation between all management sections (Users, Storages, Namespaces, SQL Studio, Settings)

**SQL Studio**:
- **FR-014**: System MUST provide a code editor with SQL syntax highlighting
- **FR-015**: System MUST provide autocomplete suggestions for table names based on available tables
- **FR-016**: System MUST provide autocomplete suggestions for column names based on table context
- **FR-017**: System MUST execute SQL queries and return results in a structured format
- **FR-018**: System MUST display query results in a sortable, paginated data grid
- **FR-019**: System MUST display clear error messages for failed queries
- **FR-020**: System MUST limit query results to a maximum of 10,000 rows, displaying a warning when limit is reached
- **FR-021**: System MUST enforce a 30-second query execution timeout with user-visible countdown and cancellation option

**User Management** (via SQL):
- **FR-022**: UI MUST display users by executing `SELECT * FROM system.users`
- **FR-023**: UI MUST create users by executing `INSERT INTO system.users`
- **FR-024**: UI MUST update users by executing `UPDATE system.users`
- **FR-025**: System MUST enforce role-based access control (user, service, dba, system roles)
- **FR-026**: System MUST prevent users from deleting their own account

**Storage & Namespace Management** (via SQL):
- **FR-027**: UI MUST display storages by executing `SELECT * FROM system.storages`
- **FR-028**: UI MUST manage namespaces via `CREATE NAMESPACE` / `DROP NAMESPACE` SQL
- **FR-029**: UI MUST display namespace/table lists via `information_schema` queries

**Storage Browser**:
- **FR-030**: UI MAY browse storage files via `SELECT * FROM system.storage_files` (if available)
- **FR-031**: UI MUST provide hierarchical navigation through storage folders
- **FR-032**: UI MUST display file metadata (name, size, modification date)

**Settings** (via SQL):
- **FR-033**: UI MUST display settings by executing `SELECT * FROM system.settings` or similar
- **FR-034**: UI MUST display settings in organized, readable format

### Key Entities

- **Access Token**: Represents an authenticated session. Contains user ID, role, expiration time. Used for stateless authentication.
- **User**: Database user account with username, hashed password, role (user/service/dba/system), and status.
- **Storage**: Configuration for data persistence location. Contains type, path, connection details, and usage metrics.
- **Namespace**: Logical container for tables. Contains name, table references, and access configuration.
- **Query Result**: Output from SQL execution. Contains column definitions, row data, and execution metadata.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can log in and receive an access token in under 3 seconds
- **SC-002**: Token-authenticated requests complete without re-entering credentials for the token's lifetime
- **SC-003**: SQL Studio autocomplete suggestions appear within 500ms of typing
- **SC-004**: Query results for typical queries (under 1000 rows) display within 2 seconds
- **SC-005**: Administrators can create a new user in under 1 minute through the UI
- **SC-006**: Storage browser can navigate folder structures with 10,000+ files without performance degradation
- **SC-007**: All management pages load and display data within 2 seconds
- **SC-008**: 95% of administrative tasks can be completed through the UI without using CLI or raw API calls

## Clarifications

### Session 2025-12-02

- Q: Which roles should be allowed to access the Admin UI? → A: Only dba and system roles
- Q: What should be the maximum number of rows returned from a single SQL query in the UI? → A: 10,000 rows (balanced usability and performance)
- Q: How should the UI handle token expiration during an active session? → A: Silent token refresh before expiration, login only if refresh fails
- Q: What should be the query execution timeout for SQL Studio? → A: 30 seconds (balanced for analytics)
- Q: Where should the UI store the access token in the browser? → A: HttpOnly cookie (server-set, XSS-resistant)

## Assumptions

- The existing KalamDB authentication system (bcrypt password hashing, RBAC) will be leveraged for the backend
- JWT tokens will be used for the access token implementation, consistent with existing token infrastructure
- The UI will be served as static files from the KalamDB backend (no separate frontend server in production)
- shadcn/ui with Tailwind CSS will provide the component foundation
- Autocomplete will use the existing schema registry to fetch table and column metadata
- The storage browser will access filesystem paths configured in storage settings (with appropriate permission checks)
