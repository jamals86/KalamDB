let wasm;

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => state.dtor(state.a, state.b));

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function makeMutClosure(arg0, arg1, dtor, f) {
    const state = { a: arg0, b: arg1, cnt: 1, dtor };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            state.dtor(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    }
}

let WASM_VECTOR_LEN = 0;

function wasm_bindgen__convert__closures_____invoke__h53534a1ef46f6383(arg0, arg1, arg2) {
    wasm.wasm_bindgen__convert__closures_____invoke__h53534a1ef46f6383(arg0, arg1, arg2);
}

function wasm_bindgen__convert__closures_____invoke__hedbd134adbce07f2(arg0, arg1) {
    wasm.wasm_bindgen__convert__closures_____invoke__hedbd134adbce07f2(arg0, arg1);
}

function wasm_bindgen__convert__closures_____invoke__hd3a7b72008ab54d8(arg0, arg1, arg2) {
    wasm.wasm_bindgen__convert__closures_____invoke__hd3a7b72008ab54d8(arg0, arg1, arg2);
}

function wasm_bindgen__convert__closures_____invoke__hc196795d34a3c3f6(arg0, arg1, arg2, arg3) {
    wasm.wasm_bindgen__convert__closures_____invoke__hc196795d34a3c3f6(arg0, arg1, arg2, arg3);
}

const __wbindgen_enum_BinaryType = ["blob", "arraybuffer"];

const __wbindgen_enum_RequestMode = ["same-origin", "no-cors", "cors", "navigate"];

const KalamClientFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_kalamclient_free(ptr >>> 0, 1));

/**
 * WASM-compatible KalamDB client with auto-reconnection support
 *
 * Supports multiple authentication methods:
 * - Basic Auth: `new KalamClient(url, username, password)`
 * - JWT Token: `KalamClient.withJwt(url, token)`
 * - Anonymous: `KalamClient.anonymous(url)`
 *
 * # Example (JavaScript)
 * ```js
 * import init, { KalamClient, KalamClientWithJwt, KalamClientAnonymous } from './pkg/kalam_link.js';
 *
 * await init();
 *
 * // Basic Auth (username/password)
 * const client = new KalamClient(
 *   "http://localhost:8080",
 *   "username",
 *   "password"
 * );
 *
 * // JWT Token Auth
 * const jwtClient = KalamClientWithJwt.new(
 *   "http://localhost:8080",
 *   "eyJhbGciOiJIUzI1NiIs..."
 * );
 *
 * // Anonymous (localhost bypass)
 * const anonClient = KalamClientAnonymous.new("http://localhost:8080");
 *
 * // Configure auto-reconnect (enabled by default)
 * client.setAutoReconnect(true);
 * client.setReconnectDelay(1000, 30000);
 *
 * await client.connect();
 *
 * // Subscribe with options
 * const subId = await client.subscribeWithSql(
 *   "SELECT * FROM chat.messages",
 *   JSON.stringify({
 *     batch_size: 100,
 *     include_old_values: true
 *   }),
 *   (event) => console.log('Change:', event)
 * );
 * ```
 */
export class KalamClient {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(KalamClient.prototype);
        obj.__wbg_ptr = ptr;
        KalamClientFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        KalamClientFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_kalamclient_free(ptr, 0);
    }
    /**
     * Create a new KalamDB client with HTTP Basic Authentication (T042, T043, T044)
     *
     * # Arguments
     * * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
     * * `username` - Username for authentication (required)
     * * `password` - Password for authentication (required)
     *
     * # Errors
     * Returns JsValue error if url, username, or password is empty
     * @param {string} url
     * @param {string} username
     * @param {string} password
     */
    constructor(url, username, password) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(username, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(password, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_new(ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        KalamClientFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Create a new KalamDB client with JWT Token Authentication
     *
     * # Arguments
     * * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
     * * `token` - JWT token for authentication (required)
     *
     * # Errors
     * Returns JsValue error if url or token is empty
     *
     * # Example (JavaScript)
     * ```js
     * const client = KalamClient.withJwt(
     *   "http://localhost:8080",
     *   "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
     * );
     * await client.connect();
     * ```
     * @param {string} url
     * @param {string} token
     * @returns {KalamClient}
     */
    static withJwt(url, token) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(token, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_withJwt(ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return KalamClient.__wrap(ret[0]);
    }
    /**
     * Create a new KalamDB client with no authentication
     *
     * Useful for localhost connections where the server allows
     * unauthenticated access, or for development/testing scenarios.
     *
     * # Arguments
     * * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
     *
     * # Errors
     * Returns JsValue error if url is empty
     *
     * # Example (JavaScript)
     * ```js
     * const client = KalamClient.anonymous("http://localhost:8080");
     * await client.connect();
     * ```
     * @param {string} url
     * @returns {KalamClient}
     */
    static anonymous(url) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_anonymous(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return KalamClient.__wrap(ret[0]);
    }
    /**
     * Get the current authentication type
     *
     * Returns one of: "basic", "jwt", or "none"
     * @returns {string}
     */
    getAuthType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.kalamclient_getAuthType(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Enable or disable automatic reconnection
     *
     * # Arguments
     * * `enabled` - Whether to automatically reconnect on connection loss
     * @param {boolean} enabled
     */
    setAutoReconnect(enabled) {
        wasm.kalamclient_setAutoReconnect(this.__wbg_ptr, enabled);
    }
    /**
     * Set reconnection delay parameters
     *
     * # Arguments
     * * `initial_delay_ms` - Initial delay in milliseconds between reconnection attempts
     * * `max_delay_ms` - Maximum delay (for exponential backoff)
     * @param {bigint} initial_delay_ms
     * @param {bigint} max_delay_ms
     */
    setReconnectDelay(initial_delay_ms, max_delay_ms) {
        wasm.kalamclient_setReconnectDelay(this.__wbg_ptr, initial_delay_ms, max_delay_ms);
    }
    /**
     * Set maximum reconnection attempts
     *
     * # Arguments
     * * `max_attempts` - Maximum number of attempts (0 = infinite)
     * @param {number} max_attempts
     */
    setMaxReconnectAttempts(max_attempts) {
        wasm.kalamclient_setMaxReconnectAttempts(this.__wbg_ptr, max_attempts);
    }
    /**
     * Get the current reconnection attempt count
     * @returns {number}
     */
    getReconnectAttempts() {
        const ret = wasm.kalamclient_getReconnectAttempts(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Check if currently reconnecting
     * @returns {boolean}
     */
    isReconnecting() {
        const ret = wasm.kalamclient_isReconnecting(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Get the last received seq_id for a subscription
     *
     * Useful for debugging or manual resumption tracking
     * @param {string} subscription_id
     * @returns {string | undefined}
     */
    getLastSeqId(subscription_id) {
        const ptr0 = passStringToWasm0(subscription_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_getLastSeqId(this.__wbg_ptr, ptr0, len0);
        let v2;
        if (ret[0] !== 0) {
            v2 = getStringFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v2;
    }
    /**
     * Connect to KalamDB server via WebSocket (T045, T063C-T063D)
     *
     * # Returns
     * Promise that resolves when connection is established and authenticated
     * @returns {Promise<void>}
     */
    connect() {
        const ret = wasm.kalamclient_connect(this.__wbg_ptr);
        return ret;
    }
    /**
     * Disconnect from KalamDB server (T046, T063E)
     * @returns {Promise<void>}
     */
    disconnect() {
        const ret = wasm.kalamclient_disconnect(this.__wbg_ptr);
        return ret;
    }
    /**
     * Check if client is currently connected (T047)
     *
     * # Returns
     * true if WebSocket connection is active, false otherwise
     * @returns {boolean}
     */
    isConnected() {
        const ret = wasm.kalamclient_isConnected(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Insert data into a table (T048, T063G)
     *
     * # Arguments
     * * `table_name` - Name of the table to insert into
     * * `data` - JSON string representing the row data
     *
     * # Example (JavaScript)
     * ```js
     * await client.insert("todos", JSON.stringify({
     *   title: "Buy groceries",
     *   completed: false
     * }));
     * ```
     * @param {string} table_name
     * @param {string} data
     * @returns {Promise<string>}
     */
    insert(table_name, data) {
        const ptr0 = passStringToWasm0(table_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(data, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_insert(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
    /**
     * Delete a row from a table (T049, T063H)
     *
     * # Arguments
     * * `table_name` - Name of the table
     * * `row_id` - ID of the row to delete
     * @param {string} table_name
     * @param {string} row_id
     * @returns {Promise<void>}
     */
    delete(table_name, row_id) {
        const ptr0 = passStringToWasm0(table_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(row_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_delete(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
    /**
     * Execute a SQL query (T050, T063F)
     *
     * # Arguments
     * * `sql` - SQL query string
     *
     * # Returns
     * JSON string with query results
     *
     * # Example (JavaScript)
     * ```js
     * const result = await client.query("SELECT * FROM todos WHERE completed = false");
     * const data = JSON.parse(result);
     * ```
     * @param {string} sql
     * @returns {Promise<string>}
     */
    query(sql) {
        const ptr0 = passStringToWasm0(sql, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_query(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Execute a SQL query with parameters
     *
     * # Arguments
     * * `sql` - SQL query string with placeholders ($1, $2, ...)
     * * `params` - JSON array string of parameter values
     *
     * # Returns
     * JSON string with query results
     *
     * # Example (JavaScript)
     * ```js
     * const result = await client.queryWithParams(
     *   "SELECT * FROM users WHERE id = $1 AND age > $2",
     *   JSON.stringify([42, 18])
     * );
     * const data = JSON.parse(result);
     * ```
     * @param {string} sql
     * @param {string | null} [params]
     * @returns {Promise<string>}
     */
    queryWithParams(sql, params) {
        const ptr0 = passStringToWasm0(sql, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(params) ? 0 : passStringToWasm0(params, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_queryWithParams(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
    /**
     * Subscribe to table changes (T051, T063I-T063J)
     *
     * # Arguments
     * * `table_name` - Name of the table to subscribe to
     * * `callback` - JavaScript function to call when changes occur
     *
     * # Returns
     * Subscription ID for later unsubscribe
     * @param {string} table_name
     * @param {Function} callback
     * @returns {Promise<string>}
     */
    subscribe(table_name, callback) {
        const ptr0 = passStringToWasm0(table_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_subscribe(this.__wbg_ptr, ptr0, len0, callback);
        return ret;
    }
    /**
     * Subscribe to a SQL query with optional subscription options
     *
     * # Arguments
     * * `sql` - SQL SELECT query to subscribe to
     * * `options` - Optional JSON string with subscription options:
     *   - `batch_size`: Number of rows per batch (default: server-configured)
     *   - `auto_reconnect`: Override client auto-reconnect for this subscription (default: true)
     *   - `include_old_values`: Include old values in UPDATE/DELETE events (default: false)
     *   - `resume_from_seq_id`: Resume from a specific sequence ID (internal use)
     * * `callback` - JavaScript function to call when changes occur
     *
     * # Returns
     * Subscription ID for later unsubscribe
     *
     * # Example (JavaScript)
     * ```js
     * // Subscribe with options
     * const subId = await client.subscribeWithSql(
     *   "SELECT * FROM chat.messages WHERE conversation_id = 1",
     *   JSON.stringify({ batch_size: 50, include_old_values: true }),
     *   (event) => console.log('Change:', event)
     * );
     * ```
     * @param {string} sql
     * @param {string | null | undefined} options
     * @param {Function} callback
     * @returns {Promise<string>}
     */
    subscribeWithSql(sql, options, callback) {
        const ptr0 = passStringToWasm0(sql, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(options) ? 0 : passStringToWasm0(options, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_subscribeWithSql(this.__wbg_ptr, ptr0, len0, ptr1, len1, callback);
        return ret;
    }
    /**
     * Unsubscribe from table changes (T052, T063M)
     *
     * # Arguments
     * * `subscription_id` - ID returned from subscribe()
     * @param {string} subscription_id
     * @returns {Promise<void>}
     */
    unsubscribe(subscription_id) {
        const ptr0 = passStringToWasm0(subscription_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.kalamclient_unsubscribe(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
}
if (Symbol.dispose) KalamClient.prototype[Symbol.dispose] = KalamClient.prototype.free;

const EXPECTED_RESPONSE_TYPES = new Set(['basic', 'cors', 'default']);

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && EXPECTED_RESPONSE_TYPES.has(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else {
                    throw e;
                }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }
}

function __wbg_get_imports() {
    const imports = {};
    imports.wbg = {};
    imports.wbg.__wbg___wbindgen_debug_string_adfb662ae34724b6 = function(arg0, arg1) {
        const ret = debugString(arg1);
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg___wbindgen_is_function_8d400b8b1af978cd = function(arg0) {
        const ret = typeof(arg0) === 'function';
        return ret;
    };
    imports.wbg.__wbg___wbindgen_is_string_704ef9c8fc131030 = function(arg0) {
        const ret = typeof(arg0) === 'string';
        return ret;
    };
    imports.wbg.__wbg___wbindgen_is_undefined_f6b95eab589e0269 = function(arg0) {
        const ret = arg0 === undefined;
        return ret;
    };
    imports.wbg.__wbg___wbindgen_string_get_a2a31e16edf96e42 = function(arg0, arg1) {
        const obj = arg1;
        const ret = typeof(obj) === 'string' ? obj : undefined;
        var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg___wbindgen_throw_dd24417ed36fc46e = function(arg0, arg1) {
        throw new Error(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg___wbindgen_typeof_3cd6f8d7635ec871 = function(arg0) {
        const ret = typeof arg0;
        return ret;
    };
    imports.wbg.__wbg__wbg_cb_unref_87dfb5aaa0cbcea7 = function(arg0) {
        arg0._wbg_cb_unref();
    };
    imports.wbg.__wbg_addEventListener_6a82629b3d430a48 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
    }, arguments) };
    imports.wbg.__wbg_call_3020136f7a2d6e44 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = arg0.call(arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_call_abb4ff46ce38be40 = function() { return handleError(function (arg0, arg1) {
        const ret = arg0.call(arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_close_1db3952de1b5b1cf = function() { return handleError(function (arg0) {
        arg0.close();
    }, arguments) };
    imports.wbg.__wbg_code_85a811fe6ca962be = function(arg0) {
        const ret = arg0.code;
        return ret;
    };
    imports.wbg.__wbg_data_8bf4ae669a78a688 = function(arg0) {
        const ret = arg0.data;
        return ret;
    };
    imports.wbg.__wbg_fetch_8119fbf8d0e4f4d1 = function(arg0, arg1) {
        const ret = arg0.fetch(arg1);
        return ret;
    };
    imports.wbg.__wbg_get_af9dab7e9603ea93 = function() { return handleError(function (arg0, arg1) {
        const ret = Reflect.get(arg0, arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_instanceof_ArrayBuffer_f3320d2419cd0355 = function(arg0) {
        let result;
        try {
            result = arg0 instanceof ArrayBuffer;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Blob_e9c51ce33a4b6181 = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Blob;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Response_cd74d1c2ac92cb0b = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Response;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Window_b5cf7783caa68180 = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Window;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_length_22ac23eaec9d8053 = function(arg0) {
        const ret = arg0.length;
        return ret;
    };
    imports.wbg.__wbg_log_74fab41b36a979d6 = function(arg0, arg1) {
        console.log(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg_new_1ba21ce319a06297 = function() {
        const ret = new Object();
        return ret;
    };
    imports.wbg.__wbg_new_3c79b3bb1b32b7d3 = function() { return handleError(function () {
        const ret = new Headers();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_new_6421f6084cc5bc5a = function(arg0) {
        const ret = new Uint8Array(arg0);
        return ret;
    };
    imports.wbg.__wbg_new_7c30d1f874652e62 = function() { return handleError(function (arg0, arg1) {
        const ret = new WebSocket(getStringFromWasm0(arg0, arg1));
        return ret;
    }, arguments) };
    imports.wbg.__wbg_new_ff12d2b041fb48f1 = function(arg0, arg1) {
        try {
            var state0 = {a: arg0, b: arg1};
            var cb0 = (arg0, arg1) => {
                const a = state0.a;
                state0.a = 0;
                try {
                    return wasm_bindgen__convert__closures_____invoke__hc196795d34a3c3f6(a, state0.b, arg0, arg1);
                } finally {
                    state0.a = a;
                }
            };
            const ret = new Promise(cb0);
            return ret;
        } finally {
            state0.a = state0.b = 0;
        }
    };
    imports.wbg.__wbg_new_no_args_cb138f77cf6151ee = function(arg0, arg1) {
        const ret = new Function(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_new_with_str_and_init_c5748f76f5108934 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = new Request(getStringFromWasm0(arg0, arg1), arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_ok_dd98ecb60d721e20 = function(arg0) {
        const ret = arg0.ok;
        return ret;
    };
    imports.wbg.__wbg_prototypesetcall_dfe9b766cdc1f1fd = function(arg0, arg1, arg2) {
        Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
    };
    imports.wbg.__wbg_queueMicrotask_9b549dfce8865860 = function(arg0) {
        const ret = arg0.queueMicrotask;
        return ret;
    };
    imports.wbg.__wbg_queueMicrotask_fca69f5bfad613a5 = function(arg0) {
        queueMicrotask(arg0);
    };
    imports.wbg.__wbg_readyState_9d0976dcad561aa9 = function(arg0) {
        const ret = arg0.readyState;
        return ret;
    };
    imports.wbg.__wbg_reason_d4eb9e40592438c2 = function(arg0, arg1) {
        const ret = arg1.reason;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_resolve_fd5bfbaa4ce36e1e = function(arg0) {
        const ret = Promise.resolve(arg0);
        return ret;
    };
    imports.wbg.__wbg_send_7cc36bb628044281 = function() { return handleError(function (arg0, arg1, arg2) {
        arg0.send(getStringFromWasm0(arg1, arg2));
    }, arguments) };
    imports.wbg.__wbg_setTimeout_06477c23d31efef1 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = arg0.setTimeout(arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_set_425eb8b710d5beee = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
        arg0.set(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
    }, arguments) };
    imports.wbg.__wbg_set_binaryType_73e8c75df97825f8 = function(arg0, arg1) {
        arg0.binaryType = __wbindgen_enum_BinaryType[arg1];
    };
    imports.wbg.__wbg_set_body_8e743242d6076a4f = function(arg0, arg1) {
        arg0.body = arg1;
    };
    imports.wbg.__wbg_set_headers_5671cf088e114d2b = function(arg0, arg1) {
        arg0.headers = arg1;
    };
    imports.wbg.__wbg_set_method_76c69e41b3570627 = function(arg0, arg1, arg2) {
        arg0.method = getStringFromWasm0(arg1, arg2);
    };
    imports.wbg.__wbg_set_mode_611016a6818fc690 = function(arg0, arg1) {
        arg0.mode = __wbindgen_enum_RequestMode[arg1];
    };
    imports.wbg.__wbg_set_onclose_032729b3d7ed7a9e = function(arg0, arg1) {
        arg0.onclose = arg1;
    };
    imports.wbg.__wbg_set_onerror_7819daa6af176ddb = function(arg0, arg1) {
        arg0.onerror = arg1;
    };
    imports.wbg.__wbg_set_onmessage_71321d0bed69856c = function(arg0, arg1) {
        arg0.onmessage = arg1;
    };
    imports.wbg.__wbg_set_onopen_6d4abedb27ba5656 = function(arg0, arg1) {
        arg0.onopen = arg1;
    };
    imports.wbg.__wbg_static_accessor_GLOBAL_769e6b65d6557335 = function() {
        const ret = typeof global === 'undefined' ? null : global;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_GLOBAL_THIS_60cf02db4de8e1c1 = function() {
        const ret = typeof globalThis === 'undefined' ? null : globalThis;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_SELF_08f5a74c69739274 = function() {
        const ret = typeof self === 'undefined' ? null : self;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_WINDOW_a8924b26aa92d024 = function() {
        const ret = typeof window === 'undefined' ? null : window;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_status_9bfc680efca4bdfd = function(arg0) {
        const ret = arg0.status;
        return ret;
    };
    imports.wbg.__wbg_stringify_655a6390e1f5eb6b = function() { return handleError(function (arg0) {
        const ret = JSON.stringify(arg0);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_text_51046bb33d257f63 = function() { return handleError(function (arg0) {
        const ret = arg0.text();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_then_429f7caf1026411d = function(arg0, arg1, arg2) {
        const ret = arg0.then(arg1, arg2);
        return ret;
    };
    imports.wbg.__wbg_then_4f95312d68691235 = function(arg0, arg1) {
        const ret = arg0.then(arg1);
        return ret;
    };
    imports.wbg.__wbindgen_cast_0276a33a2c698888 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 146, function: Function { arguments: [NamedExternref("ErrorEvent")], shim_idx: 147, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h642479b1977f4759, wasm_bindgen__convert__closures_____invoke__h53534a1ef46f6383);
        return ret;
    };
    imports.wbg.__wbindgen_cast_0b796e23d339004c = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 146, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 147, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h642479b1977f4759, wasm_bindgen__convert__closures_____invoke__h53534a1ef46f6383);
        return ret;
    };
    imports.wbg.__wbindgen_cast_2241b6af4c4b2941 = function(arg0, arg1) {
        // Cast intrinsic for `Ref(String) -> Externref`.
        const ret = getStringFromWasm0(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbindgen_cast_2b28521819d50e8a = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 146, function: Function { arguments: [], shim_idx: 150, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h642479b1977f4759, wasm_bindgen__convert__closures_____invoke__hedbd134adbce07f2);
        return ret;
    };
    imports.wbg.__wbindgen_cast_6e52f09bd5e07686 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 146, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 147, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h642479b1977f4759, wasm_bindgen__convert__closures_____invoke__h53534a1ef46f6383);
        return ret;
    };
    imports.wbg.__wbindgen_cast_eb1693fa7a824add = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 153, function: Function { arguments: [Externref], shim_idx: 154, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h3c65cb5d3157b83f, wasm_bindgen__convert__closures_____invoke__hd3a7b72008ab54d8);
        return ret;
    };
    imports.wbg.__wbindgen_init_externref_table = function() {
        const table = wasm.__wbindgen_externrefs;
        const offset = table.grow(4);
        table.set(0, undefined);
        table.set(offset + 0, undefined);
        table.set(offset + 1, null);
        table.set(offset + 2, true);
        table.set(offset + 3, false);
    };

    return imports;
}

function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    __wbg_init.__wbindgen_wasm_module = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;


    wasm.__wbindgen_start();
    return wasm;
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (typeof module !== 'undefined') {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (typeof module_or_path !== 'undefined') {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (typeof module_or_path === 'undefined') {
        module_or_path = new URL('kalam_link_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync };
export default __wbg_init;
