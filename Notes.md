Future:
1) Alter a table and move it';s storage from storage_id to different one
2) Support changing stream table TTL via ALTER TABLE
3) Combine all the subscription logic for stream/user tables into one code base
6) Make sure _updated timestamp include also nanosecond precision
7) when reading --file todo-app.sql from the cli ignore the lines with -- since they are comments, and create a test to cover this case
8) Inside kalam-link instead of having 2 different websockets implementations to use only one which also supports the wasm target, by doing so we can reduce code duplication and have less maintenance burden
9) Implement users/roles and authentication keys using oauth or jwt tokens, this should replace the api_key thing we have now
10) in query inside where clause support comparison operators for null values, like IS NULL and IS NOT NULL
11) ✅ DONE - check in the integration tests if a flush job took took_ms = 0 then it's a failure (implemented in test_manual_flush_verification.rs tests 09, 10, 11, 12 with execute_flush_synchronously and took_ms verification)
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
31) SHOW STATS FOR TABLE app.messages;
32) Do we have counter per userId per buffered rows? this will help us tune the select from user table to check if we even need to query the buffer in first place
33) Add option for a specific usere to download all his data
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
44) Remove all X-USER-ID header we will be using oauth/basic auth or incase of system user we will add it to the auth basic auth with empty password and only from localhost only
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



Key Findings
Flush Timing Issue: Data inserted immediately before flush may not be in RocksDB column families yet, resulting in 0 rows flushed
Parquet Querying Limitation: After flush, data is removed from RocksDB but queries don't yet retrieve from Parquet files - this is a known gap


