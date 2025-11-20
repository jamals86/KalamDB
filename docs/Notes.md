Future:
1) LOW PRIORITY - Alter a table and move it's storage from storage_id to different one
2) LOW PRIORITY - Support changing stream table TTL via ALTER TABLE
3) Combine all the subscription logic for stream/user tables into one code base
6) Make sure _updated timestamp include also nanosecond precision
7) when reading --file todo-app.sql from the cli ignore the lines with -- since they are comments, and create a test to cover this case
8) Inside kalam-link instead of having 2 different websockets implementations to use only one which also supports the wasm target, by doing so we can reduce code duplication and have less maintenance burden

10) in query inside where clause support comparison operators for null values, like IS NULL and IS NOT NULL
12) In cli if a query took took_ms = 0 then it's a failure
13) In the cli add a command to show all live queries
14) In the cli add a command to kill a live query by its live id
15) for better tracking the integration tests should the names print also the folder path as well with the test name
16) for jobs add a new statuses: new, queued, running, completed, failed, retrying, cancelled
17) why we have 2 implementations for flushing: user_table_flush.rs and shared_table_flush.rs can we merge them into one? i believe they share a lot of code, we can reduce maintenance burden by having only one implementation, or can have a parent class with common code and have 2 child classes for each type of flush
18) Move all code which deals with live subscriptions into kalamdb-live module, like these codes in here: backend/crates/kalamdb-core/src/live_query
19) investigate the timestamp datatype how its being stored in rocksdb does it a string representation or binary representation? and how about the precision? is it milliseconds or nanoseconds?
20) For flushing tests first create a storage and direct all the storage into a temporary directory so we can remove it after each flush test to not leave with un-needed temporary data
22) For storing inside rocksdb as bytearray we should use protobuf instead of json
23) Add https://docs.rs/object_store/latest/object_store/ to support any object storage out there easily
24) Check if we can replace rocksdb with this one: https://github.com/foyer-rs/foyer, it already support objectstore so we can also store the non-flushed tables into s3 directly, and not forcing flushing when server goes down, even whenever we use the filesystem we can rely on the same logic inside foyer as well

31) SHOW STATS FOR TABLE app.messages; maybe this is better be implemented with information_Schemas tasks
32) Do we have counter per userId per buffered rows? this will help us tune the select from user table to check if we even need to query the buffer in first place
33) Add option for a specific user to download all his data this is done with an endpoint in rest api which will create a zip file with all his tables data in parquet format and then provide a link to download it
34) Add to the roadmap adding join which can join tables: shared<->shared, shared<->user, user<->user, user<->stream
35) Add to cli/server a version which will print the commit and build date as well which is auto-increment: add prompt instead of this one: Starting KalamDB Server v0.1.0
36) update all packages to the latest available version
37) Make the cli connect to the root user by default
38) Force always including namespace to the queries alongside the tableName
39) Whenever we create a table we can create it with a specific access policy: public, private, protected
40) Add to the cli a command session which will show the current user all info, namespace, and other session related info.
41) storing credentials in kalamdb-credentials alongside the url of the database
42) CLI should also support a regular user as well and not only the root user
43) Whenever a user send a query/sql statement first of all we check the role he has if he is creating create/alter tables then we first check the user role before we display an error like: namespace does not exists, maybe its better to include in these CREATE/ALTER sql also which roles can access them so we dont read data from untrusted users its a sensitive topic.
45) "system_users" is repeated so many times in the code base we should use a column family enum for all of them, and making all of them as Partition::new("system_users") instead of hardcoding the string multiple times, we already have SystemTable enum we can add ColumnFamily as well
46) nodeId should be unique and used from the config file and use NodeId enum
47) No need to map developer to service role we need to use only Role's values
48) make sure we use TableAccess
49) execute_create_table need to take namespaceId currently it creates inside default namespace only, no need to have default namespaceid any place
50) anonymous user shouldnt be allowed to create tables or do anything except select from public tables
51) Combine all providers into one commong code: backend/crates/kalamdb-core/src/tables/shared_table_provider.rs, backend/crates/kalamdb-core/src/tables/user_table_provider.rs, backend/crates/kalamdb-core/src/tables/stream_table_provider.rs,system_table_provider.rs
52)         namespace_id: &str, table_name: &str, to NamespaceId, TableName
53) IMPORTANT: Add a story about the need for giving ability to subscribe for: * which means all users tables at once, this is done by the ai agent which listen to all the user messages at once, add also ability to listen per storageId, for this we need to add to the user message key a userId:rowId:storageId
54) Mention in the README.md that instead of using redis/messaging system/database you can use one for all of these, and subscribing directly to where your messages are stored in an easy way
55) Check the queries coming and scan for vulnerability limit the string content length
56) Add to README.md
    - Vector database
    - Vector Search
    - Has a deep design for how they should be and also update TASKS.MD and the design here


63) check for each system table if the results returned cover all the columns defined in the TableSchema
65) Add tests to cover the droping table and cleanup inside jobs table as well
66) Make sure actions like: drop/export/import/flush is having jobs rows when they finishes (TODO: Also check what kind of jobs we have)
67) test each role the actions he can do and cannot do, to cover the rbac system well, this should be done from the cli
68) A service user can also create other regular users
69) Server click ctrl+z two times will force kill even if it's still flushing or doing some job
70) Check cleaning up completed jobs, we already have a config of how long should we retain them
71) When flushing user table flush only the user who is requesting the flush to happen
72) Whenever we drop the namespace remove all tables under it
73) Test creating different users and checking each operation they can do from cli integration tests
74) check if we are using system/kalam catalogs correctly in the datafusion sessions
ctx.register_catalog("system", Arc::new(SystemCatalogProvider::new()));
ctx.register_catalog("app", Arc::new(AppCatalogProvider::new(namespace)));
Also check that registering ctaalogs are done in one place and one session, we shall combine everywhere were we register sessions and ctalogs into one place

75) Fix cli highlight select statements
76) Fix auto-complete in cli
77) Flush table job got stuck for a long time, need to investigate why and also why the tests don't detect this issue and still passes?!
78) Support an endpoint for exporting user data as a whole in a zip file
79) Need to scan the code to make it more lighweight and less dependencies, by doing that we can lower the file binary size and also memory consumption
80) More integration tests inside cli tool which covers importing sql files with multiple statements
81) CLI - add integration tests which cover a real-life use case for chat app with an ai where we have conversations and messages between users and ai agents, real-time streams for ai thinking/thoughts/typing/user online/offline status, and also flushing messages to parquet files and reloading them back
82) make sure rocksdb is only used inside kalamdb-store
83) Make sure the sql engine works on the schemas first and from there he can get the actual data of the tables
84) Divide backend/crates/kalamdb-core/src/sql/executor.rs into multiple files for better maintainability
85) Jobs model add NamespaceId type
86) Make node_id: String, into NodeId type
87) implement a service which answer these kind of things: fn get_memory_usage_mb() and will be used in stats/jobs memory/cpu and other places when needed
89) When deleting a namespace check if all tables are deleted, re-create the namespace again and check if the tables still exists there, and try to cvreate the same table again and check if it works correctly
90) Create/alter table support table doesnt return the actual rows affected count
91) If i set the logging to info inside config.toml i still see debug logs: level = "info"
92) Check the datatypes converting between rust to arrow datatypes and to rocksdb its named json i dont want to use json for this, i want fast serdes for datatypes, maybe util.rs need to manage both serialize from parquet to arrow arrow to parquet both wys currently its located inside flush folder
93) Add a new dataType which preserve the timezone info when storing timestamp with timezone
95) while: [2025-11-01 23:55:16.242] [INFO ] - main - kalamdb_server::lifecycle:413 - Waiting up to 300s for active flush jobs to complete...
display what active jobs we are waiting on
97) check if we have duplicates backend/crates/kalamdb-commons/src/constants.rs and backend/crates/kalamdb-commons/src/system_tables.rs both have system table names defined
98) IMPORTANT - If no primary key found for a table then we will add our own system column _id to be primary key with snowflake id, make sure this is implemented everywhere correctly
If the user already specified primary key then we dont do that, the _id we add also should check if the id is indeed unique as well
99) We are still using NodeId::from(format!("node-{}", std::process::id())) in multiple places we should use the same nodeId from config or context everywhere
100) JobId need to be shorter its now using timestamp and uuid which is too long, we can use namespace-table-timestamp or even a snowflake id

102) CLI Tests common - Verify that we have a timeout set while we wait for the subscription changes/notifications
103) Check to see any libraries/dependencies not needed and rmeove them, check each one of the dependencies
104) backend\crates\kalamdb-core\src\tables\system\tables_v2 is not needed anymore we have schemas which stores the tables/columns, all should be located in the new folder: backend\crates\kalamdb-core\src\tables\system\schemas
105) when we have Waiting up to 300s for active flush jobs to complete... and the user click CTRL+C again it will force the stopping and mark those jobs as failed with the right error

107) Check if we can here combine this with our main cache: backend\crates\kalamdb-core\src\sql\registry.rs, or if this needed anymore?
108) Prevent creating namespace with names like: sys/system/root/kalamdb/kalam/main/default/sql and name these as SYSTEM_RESERVED_NAMES, also add function to Namespaceid.isSystem() to check if the namespace is a system one
109) why do we need backend/crates/kalamdb-auth/src/user_repo.rs anymore? we have kalamdb-core/src/auth/user_repo.rs
110) Instead of having system.<system table> we can use sys.<system table> for less typing and easier to remember
111) Add virtualTables module to kalamdb-core/src/tables/virtual_tables to include all virtual tables for example information_schema and other virtual tables we may have in the future, virtual tables should be also registered with schema registry
112) In JobType add another model for each type with the parameters it should have in the Json in this way we can validate it easily by deserializing into the right struct for each job type
113) Check if we need to remove ColumnFamilyManager
114) Make sure when we are writing to a table with secondary index we do it in a transaction style like this:
Transaction:
  1️⃣ Insert actual value into main table (or CF)
  2️⃣ Insert corresponding key/value into secondary index CF
  3️⃣ Commit atomically
115) we are having so much AppContext:get() calls in schema_registry add it to the struct as a member and use it in all methods
116) some places we have something like this: let storage_id = StorageId::from(statement.storage_id.as_str()); its not needed since storage_id is already a StorageId type
and things like this:
        let name = statement.name.as_str();
        let namespace_id = NamespaceId::new(name);
117) Make the link client send a X-Request-ID header with a unique id per request for better tracing and debugging
118) Add to kalamdb-link client the ability to set custom headers for each request
120) Now configs are centralized inside AppContext and accessible everywhere easily, we need to check:
  - All places where we read config from file directly and change them to read from AppContext
  - Remove any duplicate config models which is a dto and use only the configs instead of mirroring it to different structs

122) in impl JobExecutor for FlushExecutor add generic to the model instead of having json parameters we can have T: DeserializeOwned + Send + Sync + 'static and then we can deserialize into the right struct directly instead of having to parse json each time

123) Query users table doesnt return _deleted columns only the deleted_at date

126) Combine the 2 shared/user tables flushing and querying using a shared hybrid service which is used in both cases to reduce code duplication and maintenance burden
both of them read from a path and from a store but user table filter the store which is the hot storage based on user id as well


128) IMPORTANT - Insert a row as a system/service user to a different user_id this will be used for users to send messages to others as well or an ai send to different user or publish to different user's stream table
INSERT INTO <namespace>.<table>
   [AS USER '<user_id>']
   VALUES (...);


130) We need to have describe table <namespace>.<table> to show the table schema in cli and server as well also to display: cold rows count, hot rows count, total rows count, storage id, primary key(s), indexes, etc

131) InitialDataOptions and InitialDataResult should now have SeqId instead of timestamp we check by it now
132) add_row_id_column should be removed and all the _updated, _id should be removed from everywhere even deprecation shouldnt be added remove the code completely
133)  _row_id: &str shouldnt be there we now use SharedTableRowId or UserTableRowId in all places instead of string
134) Instead of passing namespace/table_name and also tableid pass only TableId also places where there is both of NamespaceId and TableName pass TableId instead  
    namespace_id: &NamespaceId, //TODO: Remove we have TableId
    table_name: &TableName, //TODO: Remove we have TableId
    table_id: &TableId,

135) There is some places were we have self.app_context and we at the same time refetch the app_context again

136) Cleanup old data from: backend/crates/kalamdb-commons/src/string_interner.rs and also remove everything have to do with old system columns: _row_id, _id, _updated and also have one place which is SystemColumnNames which is in commons to have the word _seq/_deleted in, so we can refer to it from one place only

137) SharedTableFlushJob AND UserTableFlushJob have so much code duplication we need to combine them into one flush job with some parameters to differ between user/shared table flushing

138) Split into more crates - look at the file crates-splitting.md for more info

139) Instead of using JsonValue for the fields use arrow Array directly for better performance and less serdes overhead: HashMap<String, ScalarValue> should solve this issue completely.
then we wont be needing: json_to_scalar_value
SqlRequest will use the same thing as well
ColumnDefault will use ScalarValue directly as well
FilterPredicate will use ScalarValue directly as well
json_rows_to_arrow_batch will be removed completely or less code since we will be using arrow arrays directly
scalar_value_to_json will be removed completely as well
ServerMessage will use arrow arrays directly as well


140) can we get rid of using EntityStore:: and directlky use the desired store? it will be more type safe
141) Add a doc file for crates and backend server dependency graph in: docs\architecture, base it on the current code design and add to AGENTS.md to always update the archeticture, this is the base spec for the last change: crates-splitting.md

142) make get_storage_path return PathBuf and any function which responsible for paths and storage should be the same using PathBuf instead of String for better path handling across different OSes, resolve_storage_path_for_user,     pub storage_path_template: String, 

143) Add type-safe modles to ChangeNotification
144) Shared tables should have a manifest stored in memory as well for better performance with also the same as user table to disk 
145) User tables manifest should be in rocksdb and persisted into storage disk after flush
146) Instead of these let partition_name = format!(
                "{}{}:{}",
                ColumnFamilyNames::USER_TABLE_PREFIX,
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
    read the partion as it is from a const or static function in commons for all of them

147) When flushing shouldnt i do that async? so i dont block while waiting for rocksdb flushing or deleting folder to finish

148) Create a compiler for paths templates to avoid doing string replacements each time we need to get the path for a user/table
    we can combine all the places and use this compiler even in tests as well

    there is many places where we need to compile paths:
    1) In User Table output batch parquet file
    2) In Shared Table parquet path
    3) Caching the storage paths for user tables/shared tables
    4) Manifest paths for user/shared tables
    5) All error logic will be checked in the compiler, it should be optomized as much as possible
    6) We can get tableid and also namespaceId/tablename so whenever we are calling it we can get whatever method we need instead of always combining or separating it to fit
    7) Make sure manifest.json file should be next to the other parquet files in the same folder


149)  We currently check if not exists we need to use this boolean for that not the contains one,
    pub if_not_exists: bool,

150) the expiration or ttl in stream tables is not working at all, i subscribe after 30 seconds and still have the same rows returned to me

151) Add field and tablename naming convention and system reserved words like you can't create namespace with name system/sys/root/kalamdb/main/default etc or field names with _row_id/_id/_updated etc or anything which starts with _ or spaces or special characters
Create a validation function to be used in create/alter table for namespace/table/column names
The reserved words should be in commons in one place for the whole codebase to refer to it from one place only
Also make sure we dont add the "id" column as we used to do before, we rely solely on _seq system column for uniqueness and rely on the user's own fields adding

153) SharedTableFlushJob and UserTableFlushJob should take as an input the appContext and the TableId and TableType as an input for creating them

155) Whenever we drop a table cancel all jobs for this table which are still running or queued

156) When there is an sql parsing or any error the parser should return a clear error with maybe line or description of what is it
instead of: 1 failed: Invalid operation: No handler registered for statement type 'UNKNOWN'


157) Are we closing all ParquetWriter? whenever we use them?
158) extract_seq_bounds is duplicated we cna combine it
159) Add tests to cover the indexes and manifest reading - check if it's actually working and the planner works with indexes now and doesnt read the un-needed parquet files

160) I can see stuck websockets in the server, make sure we have a timeout if no ping recieved from the client for a specific time we close the connection
[2025-11-17 18:52:28.766] [INFO ] - actix-rt|system:0|arbiter:7 - actix_web::middleware::logger:450 - 127.0.0.1 "GET /v1/ws HTTP/1.1" 101 13659 "-" "-" 69671.679608
[2025-11-17 18:52:28.766] [INFO ] - actix-rt|system:0|arbiter:8 - actix_web::middleware::logger:450 - 127.0.0.1 "GET /v1/ws HTTP/1.1" 101 13323 "-" "-" 68830.506443
[2025-11-17 18:52:28.768] [INFO ] - actix-rt|system:0|arbiter:7 - kalamdb_api::actors::ws_session:178 - WebSocket connection closed: baf68a9f-ff08-4ba9-a433-80174dad0d73
[2025-11-17 18:52:28.768] [INFO ] - actix-rt|system:0|arbiter:8 - kalamdb_api::actors::ws_session:178 - WebSocket connection closed: 81013e3a-e61c-4880-9b2d-eea92a336e3a

162) Whenever we shutdown the server we force all subscrioptions to be closed and websockets as well gracefully with an event set to the user

163) If there is any stuck live_query when starting the server clear them all, might be the server crashed without graceful shutdown

164) Instead of doing this:         let table_name = format!(
            "{}.{}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );
      add function which returns this full name directly from TableId

165) No need to have tableType in tableOptions since we already have it as column

166) Add cli clear command which clears the console previous output
167) create a cli test to cover all InitialDataOptions and InitialDataResult for stream tables/users table as well
  - last rows limit
  - with multiple batches
  - specify batch size and verify it works correctly
  - with seq bounds
  - with time bounds in streams with ttl passed
  - Check the order of the recieved rows is correct in the batch
  - Check if the rows is correct in the changes notification as well



168) CurrentUserFunction is not being registered corectly it should look at the sessionstate and not like this: CurrentUserFunction::with_user_id(uid.as_str()) so we register it one time and it read from the sessionstate directly

169) clear_plan_cache should be called in any DDL that is happening
170) Make a way to set the namespace once per session and then we can use it in the next queries




Here’s the updated 5-line spec with embedding storage inside Parquet and managed HNSW indexing (with delete handling):
	1.	Parquet Storage: All embeddings are stored as regular columns in the Parquet file alongside other table columns to keep data unified and versioned per batch.
	2.	Temp Indexing: On each row insert/update, serialize embeddings into a temporary .hnsw file under /tmp/kalamdb/{namespace}/{table}/{column}-hot_index.hnsw for fast incremental indexing.
	3.	Flush Behavior: During table flush, if {table}/{column}-index.hnsw doesn’t exist, create it from all embeddings in the Parquet batches; otherwise, load and append new vectors while marking any deleted rows in the index.
	4.	Search Integration: Register a DataFusion scalar function vector_search(column, query_vector, top_k) that loads the HNSW index, filters out deleted entries, and returns nearest row IDs + distances.
	5.	Job System Hook: Add an async background IndexUpdateJob triggered post-flush to merge temporary indexes, apply deletions, and update last_indexed_batch metadata for each table column.



IMPORTANT:
1) Done - Schema information_schema
2) Done - Datatypes for columns
3) Parametrized Queries needs to work with ScalarValue and be added to the api endpoint
4) Add manifest file for each user table, that will help us locate which parquet files we need to read in each query, and if in fact we need to read parquet files at all, since sometimes the data will be only inside rocksdb and no need for file io
4) Done - Support update/deleted as a separate join table per user by MAX(_updated)
5) Storage files compaction
6) AS USER support for DML statements - to be able to insert/update/delete as a specific user_id (Only service/admin roles can do that)
7) Vector Search + HNSW indexing with deletes support
8) Now configs are centralized inside AppContext and accessible everywhere easily, we need to check:
  - All places where we read config from file directly and change them to read from AppContext
  - Remove any duplicate config models which is a dto and use only the configs instead of mirroring it to different structs

9) in impl JobExecutor for FlushExecutor add generic to the model instead of having json parameters we can have T: DeserializeOwned + Send + Sync + 'static and then we can deserialize into the right struct directly instead of having to parse json each time

10) use hashbrown instead of hashmap for better performance where possible
11) Investigate using vortex instead of parquet or as an option for the user to choose which format to use for storing flushed data


Key Findings
Flush Timing Issue: Data inserted immediately before flush may not be in RocksDB column families yet, resulting in 0 rows flushed
Parquet Querying Limitation: After flush, data is removed from RocksDB but queries don't yet retrieve from Parquet files - this is a known gap




Code Cleanup Operations:
1) Remove all occurrences of _row_id, _id, _updated system columns from the codebase
2) Replace all instances of String types for namespace/table names with their respective NamespaceId/TableName
3) Instead of passing to a method both NamespaceId and TableName, pass only TableId
4) Make sure all using UserId/NamespaceId/TableName/TableId/StorageId types instead of raw strings across the codebase
5) Instead of using "_seq","_deleted" use a SystemColumnNames constant or static function to get the name from one place only
6) Remove un-needed imports across the codebase
7) Fix all clippy warnings and errors
8) Check where we use AppContext::get() multiple times in the same struct and make it a member of the struct instead, or if the code already have AppContext as a member use it directly


