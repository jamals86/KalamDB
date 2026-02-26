import 'dart:async';
import 'dart:convert';

import 'auth.dart';
import 'models.dart';
import 'generated/api.dart' as bridge;
import 'generated/models.dart' as gen;
import 'generated/frb_generated.dart';

/// KalamDB client for Dart and Flutter.
///
/// Provides query execution, live subscriptions, authentication,
/// and server health/setup endpoints.
///
/// ```dart
/// final client = await KalamClient.connect(
///   url: 'https://db.example.com',
///   auth: Auth.basic('alice', 'secret123'),
/// );
///
/// final result = await client.query('SELECT * FROM users LIMIT 10');
/// print(result.rows);
///
/// await client.dispose();
/// ```
class KalamClient {
  final bridge.DartKalamClient _handle;
  final ConnectionHandlers? _connectionHandlers;
  var _isDisposed = false;
  Future<void>? _connectionEventPump;

  KalamClient._(this._handle, this._connectionHandlers);

  /// Initialize the Rust runtime. Call once at app startup before any
  /// [KalamClient.connect] calls.
  ///
  /// ```dart
  /// void main() async {
  ///   WidgetsFlutterBinding.ensureInitialized();
  ///   await KalamClient.init();
  ///   runApp(MyApp());
  /// }
  /// ```
  static Future<void> init() => RustLib.init();

  /// Connect to a KalamDB server.
  ///
  /// * [url] — server URL, e.g. `"https://db.example.com"`.
  /// * [auth] — authentication method. Defaults to [Auth.none].
  /// * [timeout] — HTTP request timeout. Defaults to 30 seconds.
  /// * [maxRetries] — retry count for idempotent (SELECT) queries. Defaults to 3.
  static Future<KalamClient> connect({
    required String url,
    Auth auth = const NoAuth(),
    Duration timeout = const Duration(seconds: 30),
    int maxRetries = 3,
    ConnectionHandlers? connectionHandlers,
  }) async {
    final handle = bridge.dartCreateClient(
      baseUrl: url,
      auth: _toBridgeAuth(auth),
      timeoutMs: timeout.inMilliseconds,
      maxRetries: maxRetries,
      enableConnectionEvents: connectionHandlers?.hasAny ?? false,
    );
    final client = KalamClient._(handle, connectionHandlers);
    client._startConnectionEventPumpIfNeeded();
    return client;
  }

  // ---------------------------------------------------------------------------
  // Queries
  // ---------------------------------------------------------------------------

  /// Execute a SQL query.
  ///
  /// ```dart
  /// final res = await client.query(
  ///   r'SELECT * FROM orders WHERE user_id = $1 AND status = $2',
  ///   params: ['user-uuid-123', 'pending'],
  /// );
  /// ```
  Future<QueryResponse> query(
    String sql, {
    List<dynamic>? params,
    String? namespace,
  }) async {
    final paramsJson = params != null ? jsonEncode(params) : null;
    final resp = await bridge.dartExecuteQuery(
      client: _handle,
      sql: sql,
      paramsJson: paramsJson,
      namespace: namespace,
    );
    return _fromBridgeQueryResponse(resp);
  }

  // ---------------------------------------------------------------------------
  // Authentication
  // ---------------------------------------------------------------------------

  /// Log in with username and password.
  ///
  /// Returns tokens and user info. Use the access token with [Auth.jwt]
  /// for subsequent authenticated requests.
  Future<LoginResponse> login(String username, String password) async {
    final resp = await bridge.dartLogin(
      client: _handle,
      username: username,
      password: password,
    );
    return _fromBridgeLoginResponse(resp);
  }

  /// Refresh an expiring access token.
  Future<LoginResponse> refreshToken(String refreshToken) async {
    final resp = await bridge.dartRefreshToken(
      client: _handle,
      refreshToken: refreshToken,
    );
    return _fromBridgeLoginResponse(resp);
  }

  // ---------------------------------------------------------------------------
  // Health / Setup
  // ---------------------------------------------------------------------------

  /// Check server health (version, status, etc.).
  Future<HealthCheckResponse> healthCheck() async {
    final resp = await bridge.dartHealthCheck(client: _handle);
    return HealthCheckResponse(
      status: resp.status,
      version: resp.version,
      apiVersion: resp.apiVersion,
      buildDate: resp.buildDate,
    );
  }

  /// Check whether the server requires initial setup.
  Future<SetupStatusResponse> checkSetupStatus() async {
    final resp = await bridge.dartCheckSetupStatus(client: _handle);
    return SetupStatusResponse(
      needsSetup: resp.needsSetup,
      message: resp.message,
    );
  }

  /// Perform initial server setup (create first admin user).
  Future<ServerSetupResponse> serverSetup(ServerSetupRequest request) async {
    final resp = await bridge.dartServerSetup(
      client: _handle,
      request: gen.DartServerSetupRequest(
        username: request.username,
        password: request.password,
        rootPassword: request.rootPassword,
        email: request.email,
      ),
    );
    return ServerSetupResponse(
      message: resp.message,
      user: SetupUserInfo(
        id: resp.user.id,
        username: resp.user.username,
        role: resp.user.role,
        email: resp.user.email,
        createdAt: resp.user.createdAt,
        updatedAt: resp.user.updatedAt,
      ),
    );
  }

  // ---------------------------------------------------------------------------
  // Subscriptions
  // ---------------------------------------------------------------------------

  /// Subscribe to live changes on a SQL query.
  ///
  /// Returns a `Stream<ChangeEvent>` that emits:
  /// - [AckEvent] — subscription confirmed with schema info
  /// - [InitialDataBatch] — initial snapshot rows
  /// - [InsertEvent] / [UpdateEvent] / [DeleteEvent] — live changes
  /// - [SubscriptionError] — server-side error
  ///
  /// Cancel the subscription by cancelling the `StreamSubscription`.
  ///
  /// ```dart
  /// final stream = client.subscribe('SELECT * FROM messages');
  /// await for (final event in stream) {
  ///   switch (event) {
  ///     case InsertEvent(:final rowsJson):
  ///       print('New row: ${rowsJson.first}');
  ///     case DeleteEvent(:final oldRowsJson):
  ///       print('Deleted: ${oldRowsJson.first}');
  ///     case _:
  ///       break;
  ///   }
  /// }
  /// ```
  Stream<ChangeEvent> subscribe(
    String sql, {
    int? batchSize,
    int? lastRows,
    String? subscriptionId,
  }) {
    late bridge.DartSubscription sub;
    late StreamController<ChangeEvent> controller;
    var closed = false;

    Future<void> pullLoop() async {
      try {
        while (!closed) {
          final event = await bridge.dartSubscriptionNext(subscription: sub);
          if (event == null || closed) break;
          controller.add(_fromBridgeChangeEvent(event));
        }
      } catch (e, st) {
        if (!closed) controller.addError(e, st);
      } finally {
        if (!closed) await controller.close();
      }
    }

    controller = StreamController<ChangeEvent>(
      onListen: () async {
        try {
          sub = await bridge.dartSubscribe(
            client: _handle,
            sql: sql,
            config: (batchSize != null ||
                    lastRows != null ||
                    subscriptionId != null)
                ? gen.DartSubscriptionConfig(
                    sql: sql,
                    batchSize: batchSize,
                    lastRows: lastRows,
                    id: subscriptionId,
                  )
                : null,
          );
          pullLoop();
        } catch (e, st) {
          controller.addError(e, st);
          await controller.close();
        }
      },
      onCancel: () async {
        closed = true;
        try {
          await bridge.dartSubscriptionClose(subscription: sub);
        } catch (_) {}
      },
    );

    return controller.stream;
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  /// Release resources. The client should not be used after this call.
  Future<void> dispose() async {
    _isDisposed = true;
    await _connectionEventPump;
    // FRB handles dropping the Rust-side client automatically when
    // the Dart object is garbage collected. This method is provided
    // for explicit cleanup if desired.
  }

  // ---------------------------------------------------------------------------
  // Internal converters
  // ---------------------------------------------------------------------------

  static gen.DartAuthProvider _toBridgeAuth(Auth auth) {
    return switch (auth) {
      BasicAuth(:final username, :final password) =>
        gen.DartAuthProvider.basicAuth(username: username, password: password),
      JwtAuth(:final token) => gen.DartAuthProvider.jwtToken(token: token),
      NoAuth() => const gen.DartAuthProvider.none(),
    };
  }

  static QueryResponse _fromBridgeQueryResponse(gen.DartQueryResponse resp) {
    return QueryResponse(
      success: resp.success,
      results: resp.results
          .map((r) => QueryResult(
                columns: r.columns
                    .map((c) => SchemaField(
                          name: c.name,
                          dataType: c.dataType,
                          index: c.index,
                          flags: c.flags,
                        ))
                    .toList(),
                rowsJson: r.rowsJson,
                rowCount: r.rowCount,
                message: r.message,
              ))
          .toList(),
      tookMs: resp.tookMs,
      error: resp.error != null
          ? ErrorDetail(
              code: resp.error!.code,
              message: resp.error!.message,
              details: resp.error!.details,
            )
          : null,
    );
  }

  static LoginResponse _fromBridgeLoginResponse(gen.DartLoginResponse resp) {
    return LoginResponse(
      accessToken: resp.accessToken,
      refreshToken: resp.refreshToken,
      expiresAt: resp.expiresAt,
      refreshExpiresAt: resp.refreshExpiresAt,
      user: LoginUserInfo(
        id: resp.user.id,
        username: resp.user.username,
        role: resp.user.role,
        email: resp.user.email,
        createdAt: resp.user.createdAt,
        updatedAt: resp.user.updatedAt,
      ),
    );
  }

  static ChangeEvent _fromBridgeChangeEvent(gen.DartChangeEvent event) {
    return switch (event) {
      gen.DartChangeEvent_Ack(
        :final subscriptionId,
        :final totalRows,
        :final schema,
        :final batchNum,
        :final hasMore,
        :final status
      ) =>
        AckEvent(
          subscriptionId: subscriptionId,
          totalRows: totalRows,
          schema: schema
              .map((s) => SchemaField(
                    name: s.name,
                    dataType: s.dataType,
                    index: s.index,
                    flags: s.flags,
                  ))
              .toList(),
          batchNum: batchNum,
          hasMore: hasMore,
          status: status,
        ),
      gen.DartChangeEvent_InitialDataBatch(
        :final subscriptionId,
        :final rowsJson,
        :final batchNum,
        :final hasMore,
        :final status
      ) =>
        InitialDataBatch(
          subscriptionId: subscriptionId,
          rowsJson: rowsJson,
          batchNum: batchNum,
          hasMore: hasMore,
          status: status,
        ),
      gen.DartChangeEvent_Insert(:final subscriptionId, :final rowsJson) =>
        InsertEvent(subscriptionId: subscriptionId, rowsJson: rowsJson),
      gen.DartChangeEvent_Update(
        :final subscriptionId,
        :final rowsJson,
        :final oldRowsJson
      ) =>
        UpdateEvent(
            subscriptionId: subscriptionId,
            rowsJson: rowsJson,
            oldRowsJson: oldRowsJson),
      gen.DartChangeEvent_Delete(:final subscriptionId, :final oldRowsJson) =>
        DeleteEvent(subscriptionId: subscriptionId, oldRowsJson: oldRowsJson),
      gen.DartChangeEvent_Error(
        :final subscriptionId,
        :final code,
        :final message
      ) =>
        SubscriptionError(
            subscriptionId: subscriptionId, code: code, message: message),
    };
  }

  void _startConnectionEventPumpIfNeeded() {
    final handlers = _connectionHandlers;
    if (handlers == null || !handlers.hasAny || _connectionEventPump != null) {
      return;
    }

    _connectionEventPump = () async {
      try {
        while (!_isDisposed) {
          final event = await bridge.dartNextConnectionEvent(client: _handle);
          if (_isDisposed || event == null) {
            break;
          }
          handlers.dispatch(_fromBridgeConnectionEvent(event));
        }
      } catch (error) {
        if (!_isDisposed) {
          handlers.onError?.call(
            ConnectionErrorInfo(
              message: error.toString(),
              recoverable: false,
            ),
          );
        }
      }
    }();
  }

  static ConnectionEvent _fromBridgeConnectionEvent(
    gen.DartConnectionEvent event,
  ) {
    return switch (event) {
      gen.DartConnectionEvent_Connect() => const ConnectEvent(),
      gen.DartConnectionEvent_Disconnect(:final reason) => DisconnectEvent(
          reason: DisconnectReason(
            message: reason.message,
            code: reason.code,
          ),
        ),
      gen.DartConnectionEvent_Error(:final error) => ConnectionErrorEvent(
          error: ConnectionErrorInfo(
            message: error.message,
            recoverable: error.recoverable,
          ),
        ),
      gen.DartConnectionEvent_Receive(:final message) =>
        ReceiveEvent(message: message),
      gen.DartConnectionEvent_Send(:final message) =>
        SendEvent(message: message),
    };
  }
}
