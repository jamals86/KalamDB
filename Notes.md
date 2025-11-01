Future:
1) Alter a table and move it';s storage from storage_id to different one
2) Support changing stream table TTL via ALTER TABLE
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
21) support deleting rows while they are in the parquet files and have been flushed, also the update should be supported on flushed rows
22) For storing inside rocksdb as bytearray we should use protobuf instead of json
23) Add https://docs.rs/object_store/latest/object_store/ to support any object storage out there easily
24) Check if we cna replace rocksdb with this one: https://github.com/foyer-rs/foyer, it already support objectstore so we can also store the non-flushed tables into s3 directly, and not forcing flushing when server goes down, even whenever we use the filesystem we can rely on the same logic inside foyer as well

26) Low Priority - Maybe instead of _updated we can use _seq which is a snowflake id for better syncing ability accross distributed nodes
28) ✅ DONE (2025-10-27) - Namespace struct duplication - YES, was duplicated between kalamdb-core/catalog and kalamdb-commons/system. Now consolidated into single source of truth at `kalamdb-commons/src/models/system.rs`. Removed `kalamdb-core/src/catalog/namespace.rs`. All validation logic (validate_name, can_delete, increment/decrement_table_count) moved to the commons version.

29) instead of pub struct SystemTable name it KalamTable
30) Make sure the TableSchema which is stored cover everything in one model and not spread into multiple models
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


62) ✅ DONE - Add test to flush table and check if the job persisted and completed with results correctly (implemented in test_cli_flush_table and test_cli_flush_all_tables with system.jobs verification)
63) check for each system table if the results returned cover all the columns defined in the TableSchema
64) ✅ DONE - Add test to flush all and check if the job persisted and completed with results correctly (implemented in test_cli_flush_all_tables with system.jobs verification for multiple tables)
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


IMPORTANT:
1) Schema information_schema
2) Datatypes for columns
3) Parametrized Queries

Key Findings
Flush Timing Issue: Data inserted immediately before flush may not be in RocksDB column families yet, resulting in 0 rows flushed
Parquet Querying Limitation: After flush, data is removed from RocksDB but queries don't yet retrieve from Parquet files - this is a known gap




/speckit.specify I want to create a new spec which will include these main stories:
1) Now we have tables/fields/columns schemas scattered in the project in many places, i want to combine them into one place for handling the schema of each table which also include the system tables/fields
- for that i want to create simple models which is located inside commons for all information_schemas
- TableDefinition and SystemTable and InformationSchemaTable and UserTableCounter and TableSchema and ColumnDefinition and SchemaVersion should be consilidated into one folder: kalamdb-commons/src/models/schemas
- TableDefinition will have all the information about the table just as it is having now, if anything is missing like Options add them as well
- ColumnDefinition will have all the information about each field/column
  is_nullable, is_primary_key, is_partition_key, default_value, etc
- After creating these 2 models we need to refactor the whole code base to use these 2 models instead of having multiple models for each table scattered in the code base
- This will help us to have a single source of truth for the table schemas and also make it easier to maintain and extend in the future
- Also store the SchemaVersion inside TableDefinition
- Remove all the other models we dont need them anymore after the refactor we should have one source of truth then
- Consolidate all schema-related logic into the new models
- Make sure these models can be cached for performance optimization
- Update serialization/deserialization logic as needed
- Update tests to reflect the new schema models and ensure coverage
- Query information_schemas tables should use the new models to get the schema information
- show tables/describe table commands should use the new models to get the schema information
- Make sure all the codebase will use these models instead of having multiple models everywhere
- If there is a common code which we use everywhere use a common function/method for it instead of duplicating the code in multiple places

2) KalamDataType - I want to have one DataType place where i convert DataTypes from to arrow/datafusion also use this datatype inside ColumnDefinition
   - BOOLEAN: [0x01][1 byte]
   - INT: [0x02][4 bytes i32 little-endian]
   - BIGINT: [0x03][8 bytes i64 little-endian]
   - DOUBLE: [0x04][8 bytes f64 little-endian]
   - TEXT: [0x05][4 bytes length][UTF-8 bytes]
   - TIMESTAMP: [0x06][8 bytes microseconds]
   - DATE: [0x07][4 bytes days]
   - DATETIME: [0x08][8 bytes microseconds]
   - TIME: [0x08][8 bytes microseconds]
   - JSON: [0x09][4 bytes length][UTF-8 bytes]
   - BYTES: [0x0A][4 bytes length][raw bytes]

 - Name the DataType enum as KalamDataType
 - Make sure all the codebase will use this datatype instead of having multiple datatypes everywhere
 - Implement conversion functions between KalamDataType and Arrow/DataFusion DataTypes whgich shall exists in one place and convert them easily with also caching for faster lookups
    - Update serialization/deserialization logic as needed
    - Update tests to reflect the new datatype usage and ensure coverage
    - Remove any other datatypes implementations we have in the code base
    - If there is a common code which we use everywhere use a common function/method for it instead of duplicating the code in multiple places