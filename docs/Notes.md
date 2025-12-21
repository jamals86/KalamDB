# KalamDB Tasks & Notes

## 🔒 SECURITY (HIGH PRIORITY)

43) [HIGH] Whenever a user send a query/sql statement first of all we check the role he has if he is creating create/alter tables then we first check the user role before we display an error like: namespace does not exists, maybe its better to include in these CREATE/ALTER sql also which roles can access them so we dont read data from untrusted users its a sensitive topic.
50) [HIGH] anonymous user shouldnt be allowed to create tables or do anything except select from public tables
55) [HIGH] Check the queries coming and scan for vulnerability limit the string content length
108) [HIGH] Prevent creating namespace with names like: sys/system/root/kalamdb/kalam/main/default/sql and name these as SYSTEM_RESERVED_NAMES, also add function to Namespaceid.isSystem() to check if the namespace is a system one
114) [HIGH] Make sure when we are writing to a table with secondary index we do it in a transaction style like this:
Transaction:
  1️⃣ Insert actual value into main table (or CF)
  2️⃣ Insert corresponding key/value into secondary index CF
  3️⃣ Commit atomically
151) [HIGH] Add field and tablename naming convention and system reserved words like you can't create namespace with name system/sys/root/kalamdb/main/default etc or field names with _row_id/_id/_updated etc or anything which starts with _ or spaces or special characters
Create a validation function to be used in create/alter table for namespace/table/column names
The reserved words should be in commons in one place for the whole codebase to refer to it from one place only
Also make sure we dont add the "id" column as we used to do before, we rely solely on _seq system column for uniqueness and rely on the user's own fields adding
183) [HIGH] in WebSocketSession limit the size of the request coming from the client to avoid dos attacks
187) [HIGH] Add to the link libr and the sdk an ability to pass a query with parameters to avoid sql injection attacks and support caching of the sql in the backend
189) [HIGH] For subscription parsing the query should be done with datafusion even the parsing of the tableName to avoid any sql injection attacks, and re-add the projections as well and support parameters


## ⚡ PERFORMANCE & OPTIMIZATION (HIGH/MEDIUM PRIORITY)

53) [HIGH] IMPORTANT: Add a story about the need for giving ability to subscribe for: * which means all users tables at once, this is done by the ai agent which listen to all the user messages at once, add also ability to listen per storageId, for this we need to add to the user message key a userId:rowId:storageId
139) [HIGH] Instead of using JsonValue for the fields use arrow Array directly for better performance and less serdes overhead: HashMap<String, ScalarValue> should solve this issue completely.
then we wont be needing: json_to_scalar_value
SqlRequest will use the same thing as well
ColumnDefault will use ScalarValue directly as well
FilterPredicate will use ScalarValue directly as well
json_rows_to_arrow_batch will be removed completely or less code since we will be using arrow arrays directly
scalar_value_to_json will be removed completely as well
ServerMessage will use arrow arrays directly as well
147) [HIGH] When flushing shouldnt i do that async? so i dont block while waiting for rocksdb flushing or deleting folder to finish
184) [HIGH] i see when the system is idle again after a high load Open Files: 421 this is too high we need to investigate why and make sure we close all file handles correctly, add a logging or display logs when we request it to see where its leaking from

126) [MEDIUM] Combine the 2 shared/user tables flushing and querying using a shared hybrid service which is used in both cases to reduce code duplication and maintenance burden
both of them read from a path and from a store but user table filter the store which is the hot storage based on user id as well
137) [MEDIUM] SharedTableFlushJob AND UserTableFlushJob have so much code duplication we need to combine them into one flush job with some parameters to differ between user/shared table flushing
148) [MEDIUM] Create a compiler for paths templates to avoid doing string replacements each time we need to get the path for a user/table
    we can combine all the places and use this compiler even in tests as well

    there is many places where we need to compile paths:
    1) In User Table output batch parquet file
    2) In Shared Table parquet path
    3) Caching the storage paths for user tables/shared tables
    4) Manifest paths for user/shared tables
    5) All error logic will be checked in the compiler, it should be optomized as much as possible
    6) We can get tableid and also namespaceId/tablename so whenever we are calling it we can get whatever method we need instead of always combining or separating it to fit
    7) Make sure manifest.json file should be next to the other parquet files in the same folder
186) [MEDIUM] delete_by_connection_id_async should use an index in live_queries table instead of scanning all the rows to find the matching connection_id
195) [MEDIUM] we should always have a default order by column so we always have the same vlues returned in the same order, this is important for pagination as well
205) [MEDIUM] Add test which check having like 100 parquet batches per shared table and having manifest file has 100 segments and test the performance

10) [LOW] use hashbrown instead of hashmap for better performance where possible

## 🧹 CODE QUALITY & CLEANUP (HIGH/MEDIUM/LOW PRIORITY)

**High Priority:**
12) [HIGH] Check all unwrap() and expect() calls and replace them with proper error handling

**Medium Priority:**
2) [MEDIUM] Replace all instances of String types for namespace/table names with their respective NamespaceId/TableName, we should use TableId instead of passing both:        namespace: &NamespaceId,table: &TableName, like in update_manifest_after_flush
4) [MEDIUM] Make sure all using UserId/NamespaceId/TableName/TableId/StorageId types instead of raw strings across the codebase
17) [MEDIUM] why we have 2 implementations for flushing: user_table_flush.rs and shared_table_flush.rs can we merge them into one? i believe they share a lot of code, we can reduce maintenance burden by having only one implementation, or can have a parent class with common code and have 2 child classes for each type of flush
46) [MEDIUM] nodeId should be unique and used from the config file and use NodeId enum
99) [MEDIUM] We are still using NodeId::from(format!("node-{}", std::process::id())) in multiple places we should use the same nodeId from config or context everywhere
122) [MEDIUM] in impl JobExecutor for FlushExecutor add generic to the model instead of having json parameters we can have T: DeserializeOwned + Send + Sync + 'static and then we can deserialize into the right struct directly instead of having to parse json each time

**Low Priority:**
6) [LOW] Remove un-needed imports across the codebase
8) [LOW] Check where we use AppContext::get() multiple times in the same struct and make it a member of the struct instead, or if the code already have AppContext as a member use it directly
10) [LOW] Use todo!() instead of unimplemented!() where needed
11) [LOW] Remove all commented code across the codebase
13) [LOW] make sure "_seq" and "_deleted" we use the enums statics instead of strings
48) [LOW] make sure we use TableAccess
85) [LOW] Jobs model add NamespaceId type
86) [LOW] Make node_id: String, into NodeId type
97) [LOW] check if we have duplicates backend/crates/kalamdb-commons/src/constants.rs and backend/crates/kalamdb-commons/src/system_tables.rs both have system table names defined
100) [LOW] JobId need to be shorter its now using timestamp and uuid which is too long, we can use namespace-table-timestamp or even a snowflake id
103) [LOW] Check to see any libraries/dependencies not needed and rmeove them, check each one of the dependencies
113) [LOW] Check if we need to remove ColumnFamilyManager
115) [LOW] we are having so much AppContext:get() calls in schema_registry add it to the struct as a member and use it in all methods
116) [LOW] some places we have something like this: let storage_id = StorageId::from(statement.storage_id.as_str()); its not needed since storage_id is already a StorageId type
and things like this:
        let name = statement.name.as_str();
        let namespace_id = NamespaceId::new(name);
135) [LOW] There is some places were we have self.app_context and we at the same time refetch the app_context again
138) [LOW] Split into more crates - look at the file crates-splitting.md for more info
140) [LOW] can we get rid of using EntityStore:: and directlky use the desired store? it will be more type safe
141) [LOW] Add a doc file for crates and backend server dependency graph in: docs\architecture, base it on the current code design and add to AGENTS.md to always update the archeticture, this is the base spec for the last change: crates-splitting.md
143) [LOW] Add type-safe modles to ChangeNotification
146) [LOW] Instead of these let partition_name = format!(
                "{}{}:{}",
                ColumnFamilyNames::USER_TABLE_PREFIX,
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
    read the partion as it is from a const or static function in commons for all of them
149) [LOW] We currently check if not exists we need to use this boolean for that not the contains one,
    pub if_not_exists: bool,
153) [LOW] SharedTableFlushJob and UserTableFlushJob should take as an input the appContext and the TableId and TableType as an input for creating them
158) [LOW] extract_seq_bounds is duplicated we cna combine it
165) [LOW] No need to have tableType in tableOptions since we already have it as column
181) [LOW] cut the took into 3 decimal places only:
{
  "status": "success",
  "results": [
    {
      "row_count": 1,
      "columns": [],
      "message": "Inserted 1 row(s)"
    }
  ],
  "took": 1.2685000000000002
}
192) [LOW] Remove last_seen from user or update it just only once per day to avoid too many writes to rocksdb for no reason
197) [LOW] why do we have things like this? shouldnt we prevent entering if no rows?
[2025-12-13 01:51:58.957] [INFO ] - main - kalamdb_core::jobs::jobs_manager::utils:38 - [CL-a258332a4315] Job completed: Cleaned up table insert_bench_mj3iu8zz_0:single_mj3iu900_0 successfully - 0 rows deleted, 0 bytes freed
199) [LOW] change the cli history to storing the history of queries as regular queries and not base64 but keeping in mind adding quotes to preserve adding the multi-lines queries, and also replacing password on alter user to remove the password
200) [LOW] in manifest we have duplicated values id and path use only the path:
  "segments": [
    {
      "id": "batch-0.parquet",
      "path": "batch-0.parquet",
201) [LOW] optimize the manifest by doing:
{
  "table_id": {
    "namespace_id": "chat",
    "table_name": "messages"
  },
  "user_id": "root",
  "version": 5,
  "created_at": 1765787805,
  "updated_at": 1765790837,
  "segments": [
    {
      "id": "batch-0.parquet", //TODO: Not needed we are using the path now
      "path": "batch-0.parquet",
      "column_stats": {  //TODO: Change to stats
        "id": {
          "min": 258874983317667840,
          "max": 258874997628633089,
          "null_count": 0
        }
      },
      "min_seq": 258874983321862144,
      "max_seq": 258874997628633091,
      "row_count": 24,  //TODO: Change to rows
      "size_bytes": 2701,  //TODO: Change to bytes
      "created_at": 1765787814, 
      "tombstone": false
    },
  ],
  "last_sequence_number": 3 //TODO: Change to last
}

## 🧪 TESTING & CI/CD (HIGH/MEDIUM PRIORITY)

**Testing - High Priority:**
67) [HIGH] test each role the actions he can do and cannot do, to cover the rbac system well, this should be done from the cli
73) [HIGH] Test creating different users and checking each operation they can do from cli integration tests
159) [HIGH] Add tests to cover the indexes and manifest reading - check if it's actually working and the planner works with indexes now and doesnt read the un-needed parquet files
167) [HIGH] create a cli test to cover all InitialDataOptions and InitialDataResult for stream tables/users table as well
  - last rows limit
  - with multiple batches
  - specify batch size and verify it works correctly
  - with seq bounds
  - with time bounds in streams with ttl passed
  - Check the order of the recieved rows is correct in the batch
  - Check if the rows is correct in the changes notification as well
[HIGH] Make sure there is tests which insert/updte data and then check if the actual data we inserted/updated is there and exists in select then flush the data and check again if insert/update works with the flushed data in cold storage, check that insert fails when inserting a row id primary key which already exists and update do works

**Testing - Medium Priority:**
80) [MEDIUM] More integration tests inside cli tool which covers importing sql files with multiple statements
81) [MEDIUM] CLI - add integration tests which covers a real-life use case for chat app with an ai where we have conversations and messages between users and ai agents, real-time streams for ai thinking/thoughts/typing/user online/offline status, and also flushing messages to parquet files and reloading them back
102) [MEDIUM] CLI Tests common - Verify that we have a timeout set while we wait for the subscription changes/notifications
176) [MEDIUM] Add a test to chekc if we can kill a live_query and verify the user's socket got closed and user disconnected
191) [MEDIUM] In cli tests whenever we have flush directly after it check the storage files manifest.json and the parquet files if they are exists there and the size is not 0, use one function in common which helps with this if not already having one like that

**Testing - Low Priority:**
15) [LOW] for better tracking the integration tests should the names print also the folder path as well with the test name
20) [LOW] For flushing tests first create a storage and direct all the storage into a temporary directory so we can remove it after each flush test to not leave with un-needed temporary data

**CI/CD - Medium Priority:**
2) [MEDIUM] Add code coverage to the new repo

## 🚀 FEATURES - CORE FUNCTIONALITY (HIGH/MEDIUM PRIORITY)

**High Priority:**
3) [HIGH] Parametrized Queries needs to work with ScalarValue and be added to the api endpoint
14) [HIGH] Add upsert support
72) [HIGH] Whenever we drop the namespace remove all tables under it
77) [HIGH] Flush table job got stuck for a long time, need to investigate why and also why the tests don't detect this issue and still passes?!
105) [HIGH] when we have Waiting up to 300s for active flush jobs to complete... and the user click CTRL+C again it will force the stopping and mark those jobs as failed with the right error
150) [HIGH] the expiration or ttl in stream tables is not working at all, i subscribe after 30 seconds and still have the same rows returned to me
162) [HIGH] Whenever we shutdown the server we force all subscrioptions to be closed and websockets as well gracefully with an event set to the user
174) [HIGH] Make sure we flush the table before we alter it to avoid any data loss from a previous schema, also make sure we lock the table for writing/reading while it is being altered

**Medium Priority:**
3) [MEDIUM] Combine all the subscription logic for stream/user tables into one code base
5) [MEDIUM] Storage files compaction
12) [MEDIUM] aDD objectstore for storing files in s3/azure/gcs compatible storages
13) [MEDIUM] Add BEGIN TRANSACTION / COMMIT TRANSACTION support for multiple statements in one transaction, This will make the insert batch faster
39) [MEDIUM] Whenever we create a table we can create it with a specific access policy: public, private, protected
41) [MEDIUM] storing credentials in kalamdb-credentials alongside the url of the database
56) [MEDIUM] Add to README.md
    - Vector database
    - Vector Search
    - Has a deep design for how they should be and also update TASKS.MD and the design here
63) [MEDIUM] check for each system table if the results returned cover all the columns defined in the TableSchema
66) [MEDIUM] Make sure actions like: drop/export/import/flush is having jobs rows when they finishes (TODO: Also check what kind of jobs we have)
68) [MEDIUM] A service user can also create other regular users
70) [MEDIUM] Check cleaning up completed jobs, we already have a config of how long should we retain them
74) [MEDIUM] check if we are using system/kalam catalogs correctly in the datafusion sessions
ctx.register_catalog("system", Arc::new(SystemCatalogProvider::new()));
ctx.register_catalog("app", Arc::new(AppCatalogProvider::new(namespace)));
Also check that registering ctaalogs are done in one place and one session, we shall combine everywhere were we register sessions and ctalogs into one place
83) [MEDIUM] Make sure the sql engine works on the schemas first and from there he can get the actual data of the tables
89) [MEDIUM] When deleting a namespace check if all tables are deleted, re-create the namespace again and check if the tables still exists there, and try to cvreate the same table again and check if it works correctly
91) [MEDIUM] If i set the logging to info inside config.toml i still see debug logs: level = "info"
93) [MEDIUM] Add a new dataType which preserve the timezone info when storing timestamp with timezone
112) [MEDIUM] In JobType add another model for each type with the parameters it should have in the Json in this way we can validate it easily by deserializing into the right struct for each job type
117) [MEDIUM] Make the link client send a X-Request-ID header with a unique id per request for better tracing and debugging
118) [MEDIUM] Add to kalamdb-link client the ability to set custom headers for each request
130) [MEDIUM] We need to have describe table <namespace>.<table> to show the table schema in cli and server as well also to display: cold rows count, hot rows count, total rows count, storage id, primary key(s), indexes, etc
144) [MEDIUM] Shared tables should have a manifest stored in memory as well for better performance with also the same as user table to disk 
155) [MEDIUM] Whenever we drop a table cancel all jobs for this table which are still running or queued
156) [MEDIUM] When there is an sql parsing or any error the parser should return a clear error with maybe line or description of what is it
instead of: 1 failed: Invalid operation: No handler registered for statement type 'UNKNOWN'
157) [MEDIUM] Are we closing all ParquetWriter? whenever we use them?
163) [MEDIUM] If there is any stuck live_query when starting the server clear them all, might be the server crashed without graceful shutdown
169) [MEDIUM] clear_plan_cache should be called in any DDL that is happening
175) [MEDIUM] When altering a table to add/remove columns we need to update the manifest file as well
177) [MEDIUM] The loading of tables and registering its providers is scattered, i want to make it one place for on server starts and on create table
178) [MEDIUM] Flushing - Maybe we need to change the flush to look at the active manifests and from them we can know what needs to be flushed instead of scaning the whole hot rows, this way we can make sure all the manifests are flushed as well.
180) [MEDIUM] For jobs add a method for each executor called: preValidate which check if we should create that job or not, so we can check if we need to evict data or there is no need, this will not need a created job to run
190) [MEDIUM] NamespaceId should be maximum of 32 characters only to avoid long names which may cause issues in file systems
17) [MEDIUM] Persist views in the system_views table and load them on database starts
203) [MEDIUM] Can you check if we can use the manifest as an indication of having rows which needs flushing or you think its better to keep it this way which is now? if we flush and we didnt find any manifest does it fails? can you make sure this scenario is well written?

## 🎨 FEATURES - USER EXPERIENCE (MEDIUM/LOW PRIORITY)

**CLI - Medium Priority:**
13) [MEDIUM] In the cli add a command to show all live queries
14) [MEDIUM] In the cli add a command to kill a live query by its live id

**CLI - Low Priority:**
7) [LOW] when reading --file todo-app.sql from the cli ignore the lines with -- since they are comments, and create a test to cover this case
37) [LOW] Make the cli connect to the root user by default
40) [LOW] Add to the cli a command session which will show the current user all info, namespace, and other session related info.
42) [LOW] CLI should also support a regular user as well and not only the root user
75) [LOW] Fix cli highlight select statements
76) [LOW] Fix auto-complete in cli
166) [LOW] Add cli clear command which clears the console previous output
170) [LOW] Make a way to set the namespace once per session and then we can use it in the next queries

**UI - Medium Priority:**
1) [MEDIUM] The browser should fetch all namespaces/tables/columns in one query not multiple queries for better performance
3) [MEDIUM] even when there is an error display the error alone with time took display separatly, we need to be similar to the cli
10) [MEDIUM] whenever there is an error in query display only the error not the json: {"status":"error","results":[],"took":4.9274000000000004,"error":{"code":"SQL_EXECUTION_ERROR","message":"Statement 1 failed: Execution error: Schema error: No field named i44d. Valid fields are chat.messages.id, chat.messages.conversation_id, chat.messages.role_id, chat.messages.content, chat.messages.created_at, chat.messages._seq, chat.messages._deleted.","details":"select * from chat.messages where i44d = 256810499606478848"}}

**UI - Low Priority:**
2) [LOW] Display the type of table stream/shared/user tables with icon per type
9) [LOW] whenever subscribing: Live - Subscribed [2:11:04 PM] INSERT: +1 rows (total: 16) add another button which you can open a dialog to view all the mesages/logging which was recieved from the websocket subscription
11) [LOW] currently when stoping subscription the live checkbox is turned off automatically, it should stay on
12) [LOW] The table cells should be selectable when clicking on them, and on each cell you can right mouse click -> view data if it has more data there
13) [LOW] when subscribing the first column which is the type should indicate its not an actual column which the user can query its only indication what event type is this, also add to it's left side a timestamp column to indicate when this event came, and whenever a new change is added the newly added row should splash

**General - Low Priority:**
1) [LOW] Alter a table and move it's storage from storage_id to different one
2) [LOW] Support changing stream table TTL via ALTER TABLE
33) [LOW] Add option for a specific user to download all his data this is done with an endpoint in rest api which will create a zip file with all his tables data in parquet format and then provide a link to download it
34) [LOW] Add to the roadmap adding join which can join tables: shared<->shared, shared<->user, user<->user, user<->stream
35) [LOW] Add to cli/server a version which will print the commit and build date as well which is auto-increment: add prompt instead of this one: Starting KalamDB Server v0.1.0
36) [LOW] update all packages to the latest available version
38) [LOW] Force always including namespace to the queries alongside the tableName
54) [LOW] Mention in the README.md that instead of using redis/messaging system/database you can use one for all of these, and subscribing directly to where your messages are stored in an easy way
69) [LOW] Server click ctrl+z two times will force kill even if it's still flushing or doing some job
78) [LOW] Support an endpoint for exporting user data as a whole in a zip file
90) [LOW] Create/alter table support table doesnt return the actual rows affected count
95) [LOW] while: [2025-11-01 23:55:16.242] [INFO ] - main - kalamdb_server::lifecycle:413 - Waiting up to 300s for active flush jobs to complete...
display what active jobs we are waiting on
110) [LOW] Instead of having system.<system table> we can use sys.<system table> for less typing and easier to remember
123) [LOW] Query users table doesnt return _deleted columns only the deleted_at date
182) [LOW] Add to the README.md an example for managing notifications in a mobile app
188) [LOW] Check why websocket is not using the http2 protocol even if we set it in the config file

## 🔮 FUTURE FEATURES (LOW PRIORITY)

7) [LOW - FUTURE] Vector Search + HNSW indexing with deletes support
11) [LOW - FUTURE] Investigate using vortex instead of parquet or as an option for the user to choose which format to use for storing flushed data
15) [LOW - FUTURE] Support postgress protocol
16) [LOW - FUTURE] Add file DataType for storing files/blobs next to the storage parquet files
24) [LOW] Check if we can replace rocksdb with this one: https://github.com/foyer-rs/foyer, it already support objectstore so we can also store the non-flushed tables into s3 directly, and not forcing flushing when server goes down, even whenever we use the filesystem we can rely on the same logic inside foyer as well

[LOW - FUTURE FEATURE] Here's the updated 5-line spec with embedding storage inside Parquet and managed HNSW indexing (with delete handling):
	1.	Parquet Storage: All embeddings are stored as regular columns in the Parquet file alongside other table columns to keep data unified and versioned per batch.
	2.	Temp Indexing: On each row insert/update, serialize embeddings into a temporary .hnsw file under /tmp/kalamdb/{namespace}/{table}/{column}-hot_index.hnsw for fast incremental indexing.
	3.	Flush Behavior: During table flush, if {table}/{column}-index.hnsw doesn't exist, create it from all embeddings in the Parquet batches; otherwise, load and append new vectors while marking any deleted rows in the index.
	4.	Search Integration: Register a DataFusion scalar function vector_search(column, query_vector, top_k) that loads the HNSW index, filters out deleted entries, and returns nearest row IDs + distances.
	5.	Job System Hook: Add an async background IndexUpdateJob triggered post-flush to merge temporary indexes, apply deletions, and update last_indexed_batch metadata for each table column.

## ✅ COMPLETED / NOT RELEVANT

12) [NOT RELEVANT - CLI shows proper timing] In cli if a query took took_ms = 0 then it's a failure
18) [NOT RELEVANT - DONE in kalamdb-core/src/live/] Move all code which deals with live subscriptions into kalamdb-live module, like these codes in here: backend/crates/kalamdb-core/src/live_query
22) [NOT RELEVANT - Using bincode] For storing inside rocksdb as bytearray we should use protobuf instead of json
45) [NOT RELEVANT - Already done with ColumnFamilyNames] "system_users" is repeated so many times in the code base we should use a column family enum for all of them, and making all of them as Partition::new("system_users") instead of hardcoding the string multiple times, we already have SystemTable enum we can add ColumnFamily as well
47) [NOT RELEVANT - Using Role enum] No need to map developer to service role we need to use only Role's values
49) [NOT RELEVANT - Uses TableId now] execute_create_table need to take namespaceId currently it creates inside default namespace only, no need to have default namespaceid any place
82) [NOT RELEVANT - DONE] make sure rocksdb is only used inside kalamdb-store
84) [NOT RELEVANT - DONE with handlers/] Divide backend/crates/kalamdb-core/src/sql/executor.rs into multiple files for better maintainability
92) [NOT RELEVANT - Using bincode] Check the datatypes converting between rust to arrow datatypes and to rocksdb its named json i dont want to use json for this, i want fast serdes for datatypes, maybe util.rs need to manage both serialize from parquet to arrow arrow to parquet both wys currently its located inside flush folder
98) [NOT RELEVANT - Now uses _seq column] IMPORTANT - If no primary key found for a table then we will add our own system column _id to be primary key with snowflake id, make sure this is implemented everywhere correctly. If the user already specified primary key then we dont do that, the _id we add also should check if the id is indeed unique as well
104) [NOT RELEVANT - Old structure] backend\crates\kalamdb-core\src\tables\system\tables_v2 is not needed anymore we have schemas which stores the tables/columns, all should be located in the new folder: backend\crates\kalamdb-core\src\tables\system\schemas
107) [NOT RELEVANT - schema_registry consolidated] Check if we can here combine this with our main cache: backend\crates\kalamdb-core\src\sql\registry.rs, or if this needed anymore?
109) [NOT RELEVANT - Removed] why do we need backend/crates/kalamdb-auth/src/user_repo.rs anymore? we have kalamdb-core/src/auth/user_repo.rs
111) [NOT RELEVANT - Done in schema_registry/views/] Add virtualTables module to kalamdb-core/src/tables/virtual_tables to include all virtual tables for example information_schema and other virtual tables we may have in the future, virtual tables should be also registered with schema registry
128) [NOT RELEVANT - DONE with AS USER impersonation] IMPORTANT - Insert a row as a system/service user to a different user_id this will be used for users to send messages to others as well or an ai send to different user or publish to different user's stream table: INSERT INTO <namespace>.<table> [AS USER '<user_id>'] VALUES (...);
132) [NOT RELEVANT - Old columns removed, using _seq/_deleted] add_row_id_column should be removed and all the _updated, _id should be removed from everywhere even deprecation shouldnt be added remove the code completely
133) [NOT RELEVANT - Using typed RowIds] _row_id: &str shouldnt be there we now use SharedTableRowId or UserTableRowId in all places instead of string
136) [NOT RELEVANT - Old columns removed] Cleanup old data from: backend/crates/kalamdb-commons/src/string_interner.rs and also remove everything have to do with old system columns: _row_id, _id, _updated and also have one place which is SystemColumnNames which is in commons to have the word _seq/_deleted in, so we can refer to it from one place only
145) [NOT RELEVANT - Manifests implemented] User tables manifest should be in rocksdb and persisted into storage disk after flush
172) [NOT RELEVANT - Using TableId now] instead of returning: (NamespaceId, TableName) return TableId directly
179) [NOT RELEVANT - views/ exists and is the implementation] No need to have backend\crates\kalamdb-core\src\schema_registry\views since we will be impl;ementing a views which are supported by datafusion, we only persist the view create sql to be applied or run on startup of the server
204) [NOT RELEVANT - Using TableId now] we should use TableId instead of passing both:        namespace: &NamespaceId,table: &TableName, like in update_manifest_after_flush
206) [NOT RELEVANT - Using Moka cache] the last_accessed in manifest is not needed anymore since now we rely on Moka cache for knowing the last accessed time

**IMPORTANT Section - Completed:**
1) [NOT RELEVANT - DONE] Schema information_schema
2) [NOT RELEVANT - DONE] Datatypes for columns
4) [NOT RELEVANT - DONE] Add manifest file for each user table, that will help us locate which parquet files we need to read in each query, and if in fact we need to read parquet files at all, since sometimes the data will be only inside rocksdb and no need for file io
4) [NOT RELEVANT - DONE] Support update/deleted as a separate join table per user by MAX(_updated)
6) [NOT RELEVANT - DONE] AS USER support for DML statements - to be able to insert/update/delete as a specific user_id (Only service/admin roles can do that)

[NOT RELEVANT - Known issue being addressed] Key Findings
Flush Timing Issue: Data inserted immediately before flush may not be in RocksDB column families yet, resulting in 0 rows flushed
Parquet Querying Limitation: After flush, data is removed from RocksDB but queries don't yet retrieve from Parquet files - this is a known gap

---

## 📊 BENCHMARK RESULTS

┌────────────────────────────────────────────────────────────┐
│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.13s │    1532.5/s │
│  Batched (100/batch)    │   2000  │    1.84s │    1088.3/s │
│  Parallel (10 threads)  │   1000  │    0.26s │    3878.1/s │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.14s │    1445.0/s │
│  Batched (100/batch)    │   2000  │    1.63s │    1229.0/s │
│  Parallel (10 threads)  │   1000  │    0.20s │    5119.9/s │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.09s │    2260.2/s │
│  Batched (100/batch)    │   2000  │    0.03s │   73093.1/s │
│  Parallel (10 threads)  │    980  │    0.09s │   10943.2/s │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.09s │    2288.5/s  │
│  Batched (100/batch)    │   2000  │    0.04s │   51687.4/s  │
│  Parallel (10 threads)  │   1000  │    0.09s │   11409.2/s  │
└────────────────────────────────────────────────────────────┘
14) [MEDIUM] In the cli add a command to kill a live query by its live id
15) [LOW] for better tracking the integration tests should the names print also the folder path as well with the test name
17) [MEDIUM] why we have 2 implementations for flushing: user_table_flush.rs and shared_table_flush.rs can we merge them into one? i believe they share a lot of code, we can reduce maintenance burden by having only one implementation, or can have a parent class with common code and have 2 child classes for each type of flush
18) [NOT RELEVANT - DONE in kalamdb-core/src/live/] Move all code which deals with live subscriptions into kalamdb-live module, like these codes in here: backend/crates/kalamdb-core/src/live_query
20) [LOW] For flushing tests first create a storage and direct all the storage into a temporary directory so we can remove it after each flush test to not leave with un-needed temporary data
22) [NOT RELEVANT - Using bincode] For storing inside rocksdb as bytearray we should use protobuf instead of json
24) [LOW] Check if we can replace rocksdb with this one: https://github.com/foyer-rs/foyer, it already support objectstore so we can also store the non-flushed tables into s3 directly, and not forcing flushing when server goes down, even whenever we use the filesystem we can rely on the same logic inside foyer as well

33) [LOW] Add option for a specific user to download all his data this is done with an endpoint in rest api which will create a zip file with all his tables data in parquet format and then provide a link to download it
34) [LOW] Add to the roadmap adding join which can join tables: shared<->shared, shared<->user, user<->user, user<->stream
35) [LOW] Add to cli/server a version which will print the commit and build date as well which is auto-increment: add prompt instead of this one: Starting KalamDB Server v0.1.0
36) [LOW] update all packages to the latest available version
37) [LOW] Make the cli connect to the root user by default
38) [LOW] Force always including namespace to the queries alongside the tableName
39) [MEDIUM] Whenever we create a table we can create it with a specific access policy: public, private, protected
40) [LOW] Add to the cli a command session which will show the current user all info, namespace, and other session related info.
41) [MEDIUM] storing credentials in kalamdb-credentials alongside the url of the database
42) [LOW] CLI should also support a regular user as well and not only the root user
43) [HIGH] Whenever a user send a query/sql statement first of all we check the role he has if he is creating create/alter tables then we first check the user role before we display an error like: namespace does not exists, maybe its better to include in these CREATE/ALTER sql also which roles can access them so we dont read data from untrusted users its a sensitive topic.
45) [NOT RELEVANT - Already done with ColumnFamilyNames] "system_users" is repeated so many times in the code base we should use a column family enum for all of them, and making all of them as Partition::new("system_users") instead of hardcoding the string multiple times, we already have SystemTable enum we can add ColumnFamily as well
46) [MEDIUM] nodeId should be unique and used from the config file and use NodeId enum
47) [NOT RELEVANT - Using Role enum] No need to map developer to service role we need to use only Role's values
48) [LOW] make sure we use TableAccess
49) [NOT RELEVANT - Uses TableId now] execute_create_table need to take namespaceId currently it creates inside default namespace only, no need to have default namespaceid any place
50) [HIGH] anonymous user shouldnt be allowed to create tables or do anything except select from public tables
53) [HIGH] IMPORTANT: Add a story about the need for giving ability to subscribe for: * which means all users tables at once, this is done by the ai agent which listen to all the user messages at once, add also ability to listen per storageId, for this we need to add to the user message key a userId:rowId:storageId
54) [LOW] Mention in the README.md that instead of using redis/messaging system/database you can use one for all of these, and subscribing directly to where your messages are stored in an easy way
55) [HIGH] Check the queries coming and scan for vulnerability limit the string content length
56) [MEDIUM] Add to README.md
    - Vector database
    - Vector Search
    - Has a deep design for how they should be and also update TASKS.MD and the design here


63) [MEDIUM] check for each system table if the results returned cover all the columns defined in the TableSchema
66) [MEDIUM] Make sure actions like: drop/export/import/flush is having jobs rows when they finishes (TODO: Also check what kind of jobs we have)
67) [HIGH] test each role the actions he can do and cannot do, to cover the rbac system well, this should be done from the cli
68) [MEDIUM] A service user can also create other regular users
69) [LOW] Server click ctrl+z two times will force kill even if it's still flushing or doing some job
70) [MEDIUM] Check cleaning up completed jobs, we already have a config of how long should we retain them
71) [HIGH] When flushing user table flush only the user who is requesting the flush to happen
72) [HIGH] Whenever we drop the namespace remove all tables under it
73) [HIGH] Test creating different users and checking each operation they can do from cli integration tests
74) [MEDIUM] check if we are using system/kalam catalogs correctly in the datafusion sessions
ctx.register_catalog("system", Arc::new(SystemCatalogProvider::new()));
ctx.register_catalog("app", Arc::new(AppCatalogProvider::new(namespace)));
Also check that registering ctaalogs are done in one place and one session, we shall combine everywhere were we register sessions and ctalogs into one place

75) [LOW] Fix cli highlight select statements
76) [LOW] Fix auto-complete in cli
77) [HIGH] Flush table job got stuck for a long time, need to investigate why and also why the tests don't detect this issue and still passes?!
78) [LOW] Support an endpoint for exporting user data as a whole in a zip file
79) [MEDIUM] Need to scan the code to make it more lighweight and less dependencies, by doing that we can lower the file binary size and also memory consumption
80) [MEDIUM] More integration tests inside cli tool which covers importing sql files with multiple statements
81) [MEDIUM] CLI - add integration tests which cover a real-life use case for chat app with an ai where we have conversations and messages between users and ai agents, real-time streams for ai thinking/thoughts/typing/user online/offline status, and also flushing messages to parquet files and reloading them back
82) [NOT RELEVANT - DONE] make sure rocksdb is only used inside kalamdb-store
83) [MEDIUM] Make sure the sql engine works on the schemas first and from there he can get the actual data of the tables
84) [NOT RELEVANT - DONE with handlers/] Divide backend/crates/kalamdb-core/src/sql/executor.rs into multiple files for better maintainability
85) [LOW] Jobs model add NamespaceId type
86) [LOW] Make node_id: String, into NodeId type
87) [MEDIUM] implement a service which answer these kind of things: fn get_memory_usage_mb() and will be used in stats/jobs memory/cpu and other places when needed
89) [MEDIUM] When deleting a namespace check if all tables are deleted, re-create the namespace again and check if the tables still exists there, and try to cvreate the same table again and check if it works correctly
90) [LOW] Create/alter table support table doesnt return the actual rows affected count
91) [MEDIUM] If i set the logging to info inside config.toml i still see debug logs: level = "info"
92) [NOT RELEVANT - Using bincode] Check the datatypes converting between rust to arrow datatypes and to rocksdb its named json i dont want to use json for this, i want fast serdes for datatypes, maybe util.rs need to manage both serialize from parquet to arrow arrow to parquet both wys currently its located inside flush folder
93) [MEDIUM] Add a new dataType which preserve the timezone info when storing timestamp with timezone
95) [LOW] while: [2025-11-01 23:55:16.242] [INFO ] - main - kalamdb_server::lifecycle:413 - Waiting up to 300s for active flush jobs to complete...
display what active jobs we are waiting on
97) [LOW] check if we have duplicates backend/crates/kalamdb-commons/src/constants.rs and backend/crates/kalamdb-commons/src/system_tables.rs both have system table names defined
98) [NOT RELEVANT - Now uses _seq column] IMPORTANT - If no primary key found for a table then we will add our own system column _id to be primary key with snowflake id, make sure this is implemented everywhere correctly
If the user already specified primary key then we dont do that, the _id we add also should check if the id is indeed unique as well
99) [MEDIUM] We are still using NodeId::from(format!("node-{}", std::process::id())) in multiple places we should use the same nodeId from config or context everywhere
100) [LOW] JobId need to be shorter its now using timestamp and uuid which is too long, we can use namespace-table-timestamp or even a snowflake id

102) [MEDIUM] CLI Tests common - Verify that we have a timeout set while we wait for the subscription changes/notifications
103) [LOW] Check to see any libraries/dependencies not needed and rmeove them, check each one of the dependencies
104) [NOT RELEVANT - Old structure] backend\crates\kalamdb-core\src\tables\system\tables_v2 is not needed anymore we have schemas which stores the tables/columns, all should be located in the new folder: backend\crates\kalamdb-core\src\tables\system\schemas
105) [HIGH] when we have Waiting up to 300s for active flush jobs to complete... and the user click CTRL+C again it will force the stopping and mark those jobs as failed with the right error

107) [NOT RELEVANT - schema_registry consolidated] Check if we can here combine this with our main cache: backend\crates\kalamdb-core\src\sql\registry.rs, or if this needed anymore?
108) [HIGH] Prevent creating namespace with names like: sys/system/root/kalamdb/kalam/main/default/sql and name these as SYSTEM_RESERVED_NAMES, also add function to Namespaceid.isSystem() to check if the namespace is a system one
109) [NOT RELEVANT - Removed] why do we need backend/crates/kalamdb-auth/src/user_repo.rs anymore? we have kalamdb-core/src/auth/user_repo.rs
110) [LOW] Instead of having system.<system table> we can use sys.<system table> for less typing and easier to remember
111) [NOT RELEVANT - Done in schema_registry/views/] Add virtualTables module to kalamdb-core/src/tables/virtual_tables to include all virtual tables for example information_schema and other virtual tables we may have in the future, virtual tables should be also registered with schema registry
112) [MEDIUM] In JobType add another model for each type with the parameters it should have in the Json in this way we can validate it easily by deserializing into the right struct for each job type
113) [LOW] Check if we need to remove ColumnFamilyManager
114) [HIGH] Make sure when we are writing to a table with secondary index we do it in a transaction style like this:
Transaction:
  1️⃣ Insert actual value into main table (or CF)
  2️⃣ Insert corresponding key/value into secondary index CF
  3️⃣ Commit atomically
115) [LOW] we are having so much AppContext:get() calls in schema_registry add it to the struct as a member and use it in all methods
116) [LOW] some places we have something like this: let storage_id = StorageId::from(statement.storage_id.as_str()); its not needed since storage_id is already a StorageId type
and things like this:
        let name = statement.name.as_str();
        let namespace_id = NamespaceId::new(name);
117) [MEDIUM] Make the link client send a X-Request-ID header with a unique id per request for better tracing and debugging
118) [MEDIUM] Add to kalamdb-link client the ability to set custom headers for each request
120) [MEDIUM] Now configs are centralized inside AppContext and accessible everywhere easily, we need to check:
  - All places where we read config from file directly and change them to read from AppContext
  - Remove any duplicate config models which is a dto and use only the configs instead of mirroring it to different structs

122) [MEDIUM] in impl JobExecutor for FlushExecutor add generic to the model instead of having json parameters we can have T: DeserializeOwned + Send + Sync + 'static and then we can deserialize into the right struct directly instead of having to parse json each time

123) [LOW] Query users table doesnt return _deleted columns only the deleted_at date

126) [MEDIUM] Combine the 2 shared/user tables flushing and querying using a shared hybrid service which is used in both cases to reduce code duplication and maintenance burden
both of them read from a path and from a store but user table filter the store which is the hot storage based on user id as well


128) [NOT RELEVANT - DONE with AS USER impersonation] IMPORTANT - Insert a row as a system/service user to a different user_id this will be used for users to send messages to others as well or an ai send to different user or publish to different user's stream table
INSERT INTO <namespace>.<table>
   [AS USER '<user_id>']
   VALUES (...);


130) [MEDIUM] We need to have describe table <namespace>.<table> to show the table schema in cli and server as well also to display: cold rows count, hot rows count, total rows count, storage id, primary key(s), indexes, etc

132) [NOT RELEVANT - Old columns removed, using _seq/_deleted] add_row_id_column should be removed and all the _updated, _id should be removed from everywhere even deprecation shouldnt be added remove the code completely
133) [NOT RELEVANT - Using typed RowIds] _row_id: &str shouldnt be there we now use SharedTableRowId or UserTableRowId in all places instead of string
134) [MEDIUM] Instead of passing namespace/table_name and also tableid pass only TableId also places where there is both of NamespaceId and TableName pass TableId instead  
    namespace_id: &NamespaceId, //TODO: Remove we have TableId
    table_name: &TableName, //TODO: Remove we have TableId
    table_id: &TableId,

135) [LOW] There is some places were we have self.app_context and we at the same time refetch the app_context again

136) [NOT RELEVANT - Old columns removed] Cleanup old data from: backend/crates/kalamdb-commons/src/string_interner.rs and also remove everything have to do with old system columns: _row_id, _id, _updated and also have one place which is SystemColumnNames which is in commons to have the word _seq/_deleted in, so we can refer to it from one place only

137) [MEDIUM] SharedTableFlushJob AND UserTableFlushJob have so much code duplication we need to combine them into one flush job with some parameters to differ between user/shared table flushing

138) [LOW] Split into more crates - look at the file crates-splitting.md for more info

139) [HIGH] Instead of using JsonValue for the fields use arrow Array directly for better performance and less serdes overhead: HashMap<String, ScalarValue> should solve this issue completely.
then we wont be needing: json_to_scalar_value
SqlRequest will use the same thing as well
ColumnDefault will use ScalarValue directly as well
FilterPredicate will use ScalarValue directly as well
json_rows_to_arrow_batch will be removed completely or less code since we will be using arrow arrays directly
scalar_value_to_json will be removed completely as well
ServerMessage will use arrow arrays directly as well


140) [LOW] can we get rid of using EntityStore:: and directlky use the desired store? it will be more type safe
141) [LOW] Add a doc file for crates and backend server dependency graph in: docs\architecture, base it on the current code design and add to AGENTS.md to always update the archeticture, this is the base spec for the last change: crates-splitting.md

142) [MEDIUM] make get_storage_path return PathBuf and any function which responsible for paths and storage should be the same using PathBuf instead of String for better path handling across different OSes, resolve_storage_path_for_user,     pub storage_path_template: String, 

143) [LOW] Add type-safe modles to ChangeNotification
144) [MEDIUM] Shared tables should have a manifest stored in memory as well for better performance with also the same as user table to disk 
145) [NOT RELEVANT - Manifests implemented] User tables manifest should be in rocksdb and persisted into storage disk after flush
146) [LOW] Instead of these let partition_name = format!(
                "{}{}:{}",
                ColumnFamilyNames::USER_TABLE_PREFIX,
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
    read the partion as it is from a const or static function in commons for all of them

147) [HIGH] When flushing shouldnt i do that async? so i dont block while waiting for rocksdb flushing or deleting folder to finish

148) [MEDIUM] Create a compiler for paths templates to avoid doing string replacements each time we need to get the path for a user/table
    we can combine all the places and use this compiler even in tests as well

    there is many places where we need to compile paths:
    1) In User Table output batch parquet file
    2) In Shared Table parquet path
    3) Caching the storage paths for user tables/shared tables
    4) Manifest paths for user/shared tables
    5) All error logic will be checked in the compiler, it should be optomized as much as possible
    6) We can get tableid and also namespaceId/tablename so whenever we are calling it we can get whatever method we need instead of always combining or separating it to fit
    7) Make sure manifest.json file should be next to the other parquet files in the same folder


149) [LOW] We currently check if not exists we need to use this boolean for that not the contains one,
    pub if_not_exists: bool,

150) [HIGH] the expiration or ttl in stream tables is not working at all, i subscribe after 30 seconds and still have the same rows returned to me

151) [HIGH] Add field and tablename naming convention and system reserved words like you can't create namespace with name system/sys/root/kalamdb/main/default etc or field names with _row_id/_id/_updated etc or anything which starts with _ or spaces or special characters
Create a validation function to be used in create/alter table for namespace/table/column names
The reserved words should be in commons in one place for the whole codebase to refer to it from one place only
Also make sure we dont add the "id" column as we used to do before, we rely solely on _seq system column for uniqueness and rely on the user's own fields adding

153) [LOW] SharedTableFlushJob and UserTableFlushJob should take as an input the appContext and the TableId and TableType as an input for creating them

155) [MEDIUM] Whenever we drop a table cancel all jobs for this table which are still running or queued

156) [MEDIUM] When there is an sql parsing or any error the parser should return a clear error with maybe line or description of what is it
instead of: 1 failed: Invalid operation: No handler registered for statement type 'UNKNOWN'


157) [MEDIUM] Are we closing all ParquetWriter? whenever we use them?
158) [LOW] extract_seq_bounds is duplicated we cna combine it
159) [HIGH] Add tests to cover the indexes and manifest reading - check if it's actually working and the planner works with indexes now and doesnt read the un-needed parquet files

162) [HIGH] Whenever we shutdown the server we force all subscrioptions to be closed and websockets as well gracefully with an event set to the user

163) [MEDIUM] If there is any stuck live_query when starting the server clear them all, might be the server crashed without graceful shutdown

165) [LOW] No need to have tableType in tableOptions since we already have it as column

166) [LOW] Add cli clear command which clears the console previous output
167) [HIGH] create a cli test to cover all InitialDataOptions and InitialDataResult for stream tables/users table as well
  - last rows limit
  - with multiple batches
  - specify batch size and verify it works correctly
  - with seq bounds
  - with time bounds in streams with ttl passed
  - Check the order of the recieved rows is correct in the batch
  - Check if the rows is correct in the changes notification as well



169) [MEDIUM] clear_plan_cache should be called in any DDL that is happening
170) [LOW] Make a way to set the namespace once per session and then we can use it in the next queries

172) [NOT RELEVANT - Using TableId now] instead of returning: (NamespaceId, TableName) return TableId directly
174) [HIGH] Make sure we flush the table before we alter it to avoid any data loss from a previous schema, also make sure we lock the table for writing/reading while it is being altered
175) [MEDIUM] When altering a table to add/remove columns we need to update the manifest file as well
176) [MEDIUM] Add a test to chekc if we can kill a live_query and verify the user's socket got closed and user disconnected

177) [MEDIUM] The loading of tables and registering its providers is scattered, i want to make it one place for on server starts and on create table

178) [MEDIUM] Flushing - Maybe we need to change the flush to look at the active manifests and from them we can know what needs to be flushed instead of scaning the whole hot rows, this way we can make sure all the manifests are flushed as well.

179) [NOT RELEVANT - views/ exists and is the implementation] No need to have backend\crates\kalamdb-core\src\schema_registry\views since we will be impl;ementing a views which are supported by datafusion, we only persist the view create sql to be applied or run on startup of the server

180) [MEDIUM] For jobs add a method for each executor called: preValidate which check if we should create that job or not, so we can check if we need to evict data or there is no need, this will not need a created job to run


181) [LOW] cut the took into 3 decimal places only:
{
  "status": "success",
  "results": [
    {
      "row_count": 1,
      "columns": [],
      "message": "Inserted 1 row(s)"
    }
  ],
  "took": 1.2685000000000002
}

182) [LOW] Add to the README.md an example for managing notifications in a mobile app
183) [HIGH] in WebSocketSession limit the size of the request coming from the client to avoid dos attacks

184) [HIGH] i see when the system is idle again after a high load Open Files: 421 this is too high we need to investigate why and make sure we close all file handles correctly, add a logging or display logs when we request it to see where its leaking from

186) [MEDIUM] delete_by_connection_id_async should use an index in live_queries table instead of scanning all the rows to find the matching connection_id

187) [HIGH] Add to the link libr and the sdk an ability to pass a query with parameters to avoid sql injection attacks and support caching of the sql in the backend

188) [LOW] Check why websocket is not using the http2 protocol even if we set it in the config file

189) [HIGH] For subscription parsing the query should be done with datafusion even the parsing of the tableName to avoid any sql injection attacks, and re-add the projections as well and support parameters

190) [MEDIUM] NamespaceId should be maximum of 32 characters only to avoid long names which may cause issues in file systems

191) [MEDIUM] In cli tests whenever we have flush directly after it check the storage files manifest.json and the parquet files if they are exists there and the size is not 0, use one function in common which helps with this if not already having one like that

192) [LOW] Remove last_seen from user or update it just only once per day to avoid too many writes to rocksdb for no reason

194) [HIGH] Block update/insert/delete directly on system tables like users/namespaces/tables/live_queries

195) [MEDIUM] we should always have a default order by column so we always have the same vlues returned in the same order, this is important for pagination as well

197) [LOW] why do we have things like this? shouldnt we prevent entering if no rows?
[2025-12-13 01:51:58.957] [INFO ] - main - kalamdb_core::jobs::jobs_manager::utils:38 - [CL-a258332a4315] Job completed: Cleaned up table insert_bench_mj3iu8zz_0:single_mj3iu900_0 successfully - 0 rows deleted, 0 bytes freed

199) [LOW] change the cli history to storing the history of queries as regular queries and not base64 but keeping in mind adding quotes to preserve adding the multi-lines queries, and also replacing password on alter user to remove the password

200) [LOW] in manifest we have duplicated values id and path use only the path:
  "segments": [
    {
      "id": "batch-0.parquet",
      "path": "batch-0.parquet",


201) [LOW] optimize the manifest by doing:
{
  "table_id": {
    "namespace_id": "chat",
    "table_name": "messages"
  },
  "user_id": "root",
  "version": 5,
  "created_at": 1765787805,
  "updated_at": 1765790837,
  "segments": [
    {
      "id": "batch-0.parquet", //TODO: Not needed we are using the path now
      "path": "batch-0.parquet",
      "column_stats": {  //TODO: Change to stats
        "id": {
          "min": 258874983317667840,
          "max": 258874997628633089,
          "null_count": 0
        }
      },
      "min_seq": 258874983321862144,
      "max_seq": 258874997628633091,
      "row_count": 24,  //TODO: Change to rows
      "size_bytes": 2701,  //TODO: Change to bytes
      "created_at": 1765787814, 
      "tombstone": false
    },
  ],
  "last_sequence_number": 3 //TODO: Change to last
}

203) [MEDIUM] Can you check if we can use the manifest as an indication of having rows which needs flushing or you think its better to keep it this way which is now? if we flush and we didnt find any manifest does it fails? can you make sure this scenario is well written?

204) [NOT RELEVANT - Using TableId now] we should use TableId instead of passing both:        namespace: &NamespaceId,table: &TableName, like in update_manifest_after_flush


205) [MEDIUM] Add test which check having like 100 parquet batches per shared table and having manifest file has 100 segments and test the performance

206) [NOT RELEVANT - Using Moka cache] the last_accessed in manifest is not needed anymore since now we rely on Moka cache for knowing the last accessed time

[HIGH] Make sure there is tests which insert/updte data and then check if the actual data we inserted/updated is there and exists in select then flush the data and check again if insert/update works with the flushed data in cold storage, check that insert fails when inserting a row id primary key which already exists and update do works


Here’s the updated 5-line spec with embedding storage inside Parquet and managed HNSW indexing (with delete handling):
	1.	Parquet Storage: All embeddings are stored as regular columns in the Parquet file alongside other table columns to keep data unified and versioned per batch.
	2.	Temp Indexing: On each row insert/update, serialize embeddings into a temporary .hnsw file under /tmp/kalamdb/{namespace}/{table}/{column}-hot_index.hnsw for fast incremental indexing.
	3.	Flush Behavior: During table flush, if {table}/{column}-index.hnsw doesn’t exist, create it from all embeddings in the Parquet batches; otherwise, load and append new vectors while marking any deleted rows in the index.
	4.	Search Integration: Register a DataFusion scalar function vector_search(column, query_vector, top_k) that loads the HNSW index, filters out deleted entries, and returns nearest row IDs + distances.
	5.	Job System Hook: Add an async background IndexUpdateJob triggered post-flush to merge temporary indexes, apply deletions, and update last_indexed_batch metadata for each table column.



IMPORTANT:
1) [NOT RELEVANT - DONE] Schema information_schema
2) [NOT RELEVANT - DONE] Datatypes for columns
3) [HIGH] Parametrized Queries needs to work with ScalarValue and be added to the api endpoint
4) [NOT RELEVANT - DONE] Add manifest file for each user table, that will help us locate which parquet files we need to read in each query, and if in fact we need to read parquet files at all, since sometimes the data will be only inside rocksdb and no need for file io
4) [NOT RELEVANT - DONE] Support update/deleted as a separate join table per user by MAX(_updated)
5) [MEDIUM] Storage files compaction
6) [NOT RELEVANT - DONE] AS USER support for DML statements - to be able to insert/update/delete as a specific user_id (Only service/admin roles can do that)
7) [LOW - FUTURE] Vector Search + HNSW indexing with deletes support
8) Now configs are centralized inside AppContext and accessible everywhere easily, we need to check:
  - All places where we read config from file directly and change them to read from AppContext
  - Remove any duplicate config models which is a dto and use only the configs instead of mirroring it to different structs

9) [MEDIUM] Partial - In impl JobExecutor for FlushExecutor add generic to the model instead of having json parameters we can have T: DeserializeOwned + Send + Sync + 'static and then we can deserialize into the right struct directly instead of having to parse json each time

10) [LOW] use hashbrown instead of hashmap for better performance where possible
11) [LOW - FUTURE] Investigate using vortex instead of parquet or as an option for the user to choose which format to use for storing flushed data
12) [MEDIUM] aDD objectstore for storing files in s3/azure/gcs compatible storages
13) [MEDIUM] Add BEGIN TRANSACTION / COMMIT TRANSACTION support for multiple statements in one transaction, This will make the insert batch faster
14) [HIGH] Add upsert support
15) [LOW - FUTURE] Support postgress protocol
16) [LOW - FUTURE] Add file DataType for storing files/blobs next to the storage parquet files
17) [MEDIUM] Persist views in the system_views table and load them on database starts
18) [HIGH] Remove the usage of scan_all from EntityStore and replace all calls to always include filter or limit and check ability to have a stream instead of returning a vector



[NOT RELEVANT - Known issue being addressed] Key Findings
Flush Timing Issue: Data inserted immediately before flush may not be in RocksDB column families yet, resulting in 0 rows flushed
Parquet Querying Limitation: After flush, data is removed from RocksDB but queries don't yet retrieve from Parquet files - this is a known gap




Code Cleanup Operations:
2) [MEDIUM] Replace all instances of String types for namespace/table names with their respective NamespaceId/TableName, we should use TableId instead of passing both:        namespace: &NamespaceId,table: &TableName, like in update_manifest_after_flush
3) [MEDIUM] Instead of passing to a method both NamespaceId and TableName, pass only TableId
4) [MEDIUM] Make sure all using UserId/NamespaceId/TableName/TableId/StorageId types instead of raw strings across the codebase
6) [LOW] Remove un-needed imports across the codebase
7) [HIGH] Fix all clippy warnings and errors
8) [LOW] Check where we use AppContext::get() multiple times in the same struct and make it a member of the struct instead, or if the code already have AppContext as a member use it directly
9) [HIGH] Use clippy suggestions to improve code quality
10) [LOW] Use todo!() instead of unimplemented!() where needed
11) [LOW] Remove all commented code across the codebase
12) [HIGH] Check all unwrap() and expect() calls and replace them with proper error handling
13) [LOW] make sure "_seq" and "_deleted" we use the enums statics instead of strings


Tasks To Repo:
1) [HIGH] Add ci/cd pipelines to the new repo
2) [MEDIUM] Add code coverage to the new repo



│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.13s │    1532.5/s │
│  Batched (100/batch)    │   2000  │    1.84s │    1088.3/s │
│  Parallel (10 threads)  │   1000  │    0.26s │    3878.1/s │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.14s │    1445.0/s │
│  Batched (100/batch)    │   2000  │    1.63s │    1229.0/s │
│  Parallel (10 threads)  │   1000  │    0.20s │    5119.9/s │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.09s │    2260.2/s │
│  Batched (100/batch)    │   2000  │    0.03s │   73093.1/s │
│  Parallel (10 threads)  │    980  │    0.09s │   10943.2/s │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    BENCHMARK RESULTS                       │
├────────────────────────────────────────────────────────────┤
│  Test Type              │  Rows   │  Time    │  Rate       │
├────────────────────────────────────────────────────────────┤
│  Single-row inserts     │    200  │    0.09s │    2288.5/s  │
│  Batched (100/batch)    │   2000  │    0.04s │   51687.4/s  │
│  Parallel (10 threads)  │   1000  │    0.09s │   11409.2/s  │
└────────────────────────────────────────────────────────────┘

UI Changes:
1) [MEDIUM] The browser should fetch all namespaces/tables/columns in one query not multiple queries for better performance
2) [LOW] Display the type of table stream/shared/user tables with icon per type
3) [MEDIUM] even when there is an error display the error alone with time took display separatly, we need to be similar to the cli
9) [LOW] whenever subscribing: Live - Subscribed [2:11:04 PM] INSERT: +1 rows (total: 16) add another button which you can open a dialog to view all the mesages/logging which was recieved from the websocket subscription
10) [MEDIUM] whenever there is an error in query display only the error not the json: {"status":"error","results":[],"took":4.9274000000000004,"error":{"code":"SQL_EXECUTION_ERROR","message":"Statement 1 failed: Execution error: Schema error: No field named i44d. Valid fields are chat.messages.id, chat.messages.conversation_id, chat.messages.role_id, chat.messages.content, chat.messages.created_at, chat.messages._seq, chat.messages._deleted.","details":"select * from chat.messages where i44d = 256810499606478848"}}
11) [LOW] currently when stoping subscription the live checkbox is turned off automatically, it should stay on
12) [LOW] The table cells should be selectable when clicking on them, and on each cell you can right mouse click -> view data if it has more data there
13) [LOW] when subscribing the first column which is the type should indicate its not an actual column which the user can query its only indication what event type is this, also add to it's left side a timestamp column to indicate when this event came, and whenever a new change is added the newly added row should splash
