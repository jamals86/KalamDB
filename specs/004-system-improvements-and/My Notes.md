
Features:
- Add CLI tool which is using the API and you can login with specific user to do the actions inside of it, this should be a separate binary
    - This CLI Tool should use the kalamdb-client-sdk which is written in rust and will be compiled as webassembly
    - The client-sdk should also support query caching in the future (So instead than sending sql queries we can send object to prevent always parsing it)
    - For now we can only support typescript-sdk only
- Make the project has a generic serdes models, so we can change the serialization from the json
- A new table called user_files which is a table per user for storing files and their references in the storage location we have
- KFlows - Add a workflow triggers which also listen to streams, actions being done and write to the database
- KFlows will be used for system maintainance like: replication/backup and also periodict cleanup of deleted rows
- Sharding the user's own table's into multiple column families, for example: userA will be redirected to Shard A -> with users.shard-a.messages
    If we have like 5 user tables and 10 shards in total we will have 50 column families
- Raft Cluster - System tables should be replicated to all nodes
- Raft Cluster - User tables each shard should have a group in the cluster and replicated accordingly
- User can easily download his data
- Parallel writers with sharding per user
- compaction jobs that runs per user whenever he has many parquet files in the system
- Add an optimizre which runs in the background and check if the queries/design is all correctly functioning like:
    - If a table is a shared table and has many rows
    - If a user table is getting big
    - more will come later, the archeticture should support this


Tools & Utilities:
- Build as a docker container Dokerfile
- An example of running with docker-compose
- Auto-deploy to github as an executables
- 



The KalamDB should have similar sql syntax as postgress and MySQL so whenever i ask about syntax you search how Postgress and MySQL writes this syntax and do things in their architecture
Also the CLI tool for KalamDb should be similar as well
Error messages and returned syntax from SQL queries/api and cli output should be also similar
Always prefer simple and clear archeticture and logic, the the design should be simple and scalable without so much complexity its a simple database



Storages for tables:
1) Each table (shared/user) - A reference to the storage location by Id if it's not specified then we will use the local as the storage
2) the system can have multiple storages defined in it with the name local, by default when installing a new system there wil be a default storage which is ./data/storage
3) Whenever we view the storages table we should see the default first with id="local"
4) Storage table should have storageId, storageName, description, type ...
5) The type can be s3 or filesystem, should be an enum might add more in the future
6) each storage location must have 3 type of directories:
	- Storage directory: ./data/storage (For the local one we have this as empty, then we will look at the config.toml then) other cases it can be: s3://bucketId/ for example
	- Shared Tables dir (Relative to the storage dir), for example: {namespace}/shared/{tableName}
	Note: Shared Tables must have the variables in this order: {namespace} then the {tableName}
	
	- User Tables dir (Relative to the storage dir) it must contain {userId} in it, for example: {namespace}/users/{tableName}/{shard}/{userId}
	Note: User tables dir must have these variables in order: {namespace} then after it should be the {tableName} then {shard} if the user wants shards then the {userId}
7) Inside user's table add another columns:
	- storage_mode	table - which will use the tables own storage, region - which will look at the storage_id in the user's table defined here
	- storage_id 	which will be used when we have a user table with an option called
8) If a user table when it was created with an option:  "use_user_storage": true, then we will look at the user's define storage_id if it's set there to storage_mode = table then we will fallback to the user's table storage_id which was defined when creating the table
9) User's table should always have a storage_id defined in it



Notes:
#1) devide the cli test into multiple files
#2) when starting the server check first if the port is opened already ebfore laoding the rocksdb
#3) The cli show progress while the query is performing with a loading indication and time since start
#4) auto complete is not working at all in the cli, it should also work with fetching tables/schemas from select * from system.tables
#5) in the query output when selecting always preserve the order of the columns which are returned (without ruining the performance)
#6) Add log rotation
#7) specify how many logs for rocksdb to preserve
#8) fix bug in deleting a user table:
#    [2025-10-22 13:27:02.956] [WARN ] - actix-rt|system:0|arbiter:12 - kalamdb_core::services::table_deletion_service:252 - Storage path does not exist: /data/${user_id}/tables
#    [2025-10-22 13:27:02.981] [WARN ] - actix-rt|system:0|arbiter:12 - kalamdb_core::services::table_deletion_service:152 - Failed to decrement storage usage count: Not found: Storage location '/data/${user_id}/tables' not found
#9) Whenever we create a shared table directly we should create it's corresponding folder in the storage location it has been set in it to be flushed to
#10) Add a healthcheck for the kalamdb-server api
#11) add a connection check whenever we open the kalam-cli tool, if the server is down then we display an error and don't open the cli at all, this healthcheck should be added to the kalam-link
#12) add CLEAR CACHE; which clears the session caching, the query caching or any caches added in the future
#13) Remove storage_locations it's the same as system.storages also we need to add the same columns from it: credentials
#14) change the /api/sql to /v1/api/sql and also /v1/ws, /v1/api/healthcheck so that we have consistent versioning in the future
#15) divide the main.rs file into multiple files
#16) All parsers which is responsible for parsing SQL statemenets like cREATE STORAGE and other commands should be moved to kalamdb-sql
#        Should this be moved to kalamdb-sql -> backend/crates/kalamdb-core/src/sql/executor.rs
#22) In progress -   flush jobs not working it stuck running and never starts, i init the flush using flush table namespac1.files

#21) dont create flush job 2 times if there is an already running one
#23) The fields order when i run select * from table should always be consistent with the created table order of fields
#24) Why we still have a long if-else in backend/crates/kalamdb-core/src/sql/executor.rs for parsing sql's? shouldnt the parser be inside kalamdb-sql?
#    I want you to revisit all sql parsers and logic which parses sql statements and make sure we dont have a duplicated code for these
#    
#    system_schema.register_table - strill uses a string here instead of an enum
#    its better to move backend/crates/kalamdb-sql/src/parser/system.rs which has a list of all system tables into an enum inside kalamdb-commons
#29) DONE - When creating a table either it's user/shared table you should specify a storage_id for it, if not
#    then the local storage will be used for that, there is no storage_location column need to be there
#    make sure the create user/shared table can have storage_id with it
#31) parsers for the 3 commands create user/stream/shared table should be a single parser parent for all of them
#    All of them are the same except each one of them has different options/attributes in the end of the command
#    We can trim the word after the CREATE and then parse it the same way from sqlparser and preserving the type in a different value for knowing how to create it
#    Add StorageId just like UserId/NamespaceId
#    CreateSharedTableStatement/CreateUserTableStatement shares many attributes they can be emplementing extending a parent class
#    StorageLocation is not needed anymore check if we can remove it completely



23) creating table fields should support also something like this: CREATE USER TABLE app.files (
    uploaded_at TIMESTAMP DEFAULT NOW()
24) creating a table either its a user/shared/stream table must have primary key column, primaries can be BIGINT/String, and can support Snowflake for now as the auto-increment
        - id BIGINT PRIMARY KEY AUTO_INCREMENT(SNOWFLAKE),
            Supported aut-inc: SNOWFLAKE/UUID(v7) and in the future SEQUENCE
            These can be sorted and used as keys
        - MUST: stream tables must have primary with aut-inc included
25) make not null strict and check them whenever we insert/update rows
26) The fields order when i run select * from table should always be consistent with the created table order of fields


24) took_ms instead than execution_time_ms in the api response
25) in storages table instead of base_directory column name it uri
10) deleting a storage should only be done when no table is using this storage only
11) OWNER_ID 'user1' in the create user statement is not needed
    since this is a user table registered one time
    and each user when insert his data into it, will be stored into his own storage location based on the storage
12) no need for this syntax: "TABLE_TYPE shared" when creating a shared table
13) we should have types of auth into the database
    - A new column in database users for this role
    - A table can have a new column called 'access' which: Can be 'public', 'private', or 'restricted', this is only needed for shared tables to choose who can access it if the user's can or its only from a service access or only for dba's
    - It can have 4 types for now as an enum

        Role: user
        Default end-user account
        - Can SELECT from public shared tables- Can SELECT/INSERT/UPDATE/DELETE into their own user tables only- Cannot access system tables
        A normal app user


        Role: service
        Internal backend or site integration account
        - Can access shared tables and user tables for background jobs- Can trigger backups, or cleanup jobs- No CREATE/DROP of system tables
        API service or background worker

        Role: dba
        Database administrator
        - Full access to all tables including system tables- Can CREATE/ALTER/DROP namespaces, tables, storages- Can view logs, jobs, metrics
        System admin (you or operators)

        Role: system (internal only) (Localhost only)
        System-level actor (not user-visible)
        - Used by internal maintenance (compaction, flush jobs)- Can perform replication and background tasks
        Raft node replication / internal tasks











16) Job Status need to have an enum as a value so we dont make mistakes with typo's, also check other places where we might need enums as well
17) CLI - Indicate a green dot on the prompt: {dot green or red} kalam> which indicate if the server online or offline
18) CLI - Add also logging for the cli alone
19) CLI - Auto complete still not working - i prefer while the user write add the auto complete in gray after the word like ai auto complete
20) Link - Should be as much light weight as possible, currently its 400kb which can be less
25) Make a test compiling kalamdb-link into wasm and try include it into a typescript class
26) Make the logic the same as postgres which whenever insert/update/delete it returns the affected rows correctly
30) CLI - 1 row deleted. or 1 row updated. this should be returned inside the cli



Future:
1) Alter a table and move it';s storage from storage_id to different one
