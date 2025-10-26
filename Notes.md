Future:
1) Alter a table and move it';s storage from storage_id to different one
2) Support changing stream table TTL via ALTER TABLE
3) Combine all the subscription logic for stream/user tables into one code base
4) Compile a WASM build of kalam-link for browser use
5) Create a simple web UI to demo live queries and subscriptions for a Chat UI with subscribing to ai changes
6) Make sure _updated timestamp include also nanosecond precision
7) when reading --file todo-app.sql from the cli ignore the lines with -- since they are comments, and create a test to cover this case
8) 