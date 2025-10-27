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




Key Findings
Flush Timing Issue: Data inserted immediately before flush may not be in RocksDB column families yet, resulting in 0 rows flushed
Parquet Querying Limitation: After flush, data is removed from RocksDB but queries don't yet retrieve from Parquet files - this is a known gap


