import 'dart:convert';

/// Column metadata from a query result.
class SchemaField {
  final String name;
  final String dataType;
  final int index;

  /// Comma-separated flag short names, e.g. `"pk,nn,uq"`.
  /// `null` when no flags are present.
  final String? flags;

  const SchemaField({
    required this.name,
    required this.dataType,
    required this.index,
    this.flags,
  });

  /// Whether this field has the primary key flag.
  bool get isPrimaryKey => flags?.contains('pk') ?? false;

  /// Whether this field has the non-null flag.
  bool get isNonNull => flags?.contains('nn') ?? false;

  /// Whether this field has the unique flag.
  bool get isUnique => flags?.contains('uq') ?? false;
}

/// A single result set from a query execution.
class QueryResult {
  /// Column definitions.
  final List<SchemaField> columns;

  /// Raw JSON-encoded rows. Use [rows] or [toMaps] for parsed access.
  final List<String> _rowsJson;

  /// Number of rows affected or returned.
  final int rowCount;

  /// Optional message (e.g. for DDL statements).
  final String? message;

  QueryResult({
    required this.columns,
    required List<String> rowsJson,
    required this.rowCount,
    this.message,
  }) : _rowsJson = rowsJson;

  /// Parsed rows — each row is a `List<dynamic>` of column values.
  late final List<List<dynamic>> rows = _rowsJson
      .map((json) => jsonDecode(json) as List<dynamic>)
      .toList(growable: false);

  /// Rows as maps keyed by column name.
  List<Map<String, dynamic>> toMaps() {
    return rows.map((row) {
      final map = <String, dynamic>{};
      for (var i = 0; i < columns.length && i < row.length; i++) {
        map[columns[i].name] = row[i];
      }
      return map;
    }).toList(growable: false);
  }
}

/// Response from a query execution.
class QueryResponse {
  /// Whether the query succeeded.
  final bool success;

  /// Result sets (one per statement).
  final List<QueryResult> results;

  /// Execution time in milliseconds.
  final double? tookMs;

  /// Error details if the query failed.
  final ErrorDetail? error;

  const QueryResponse({
    required this.success,
    required this.results,
    this.tookMs,
    this.error,
  });

  /// Convenience: rows from the first result set, or empty.
  List<List<dynamic>> get rows =>
      results.isNotEmpty ? results.first.rows : const [];

  /// Convenience: column metadata from the first result set, or empty.
  List<SchemaField> get columns =>
      results.isNotEmpty ? results.first.columns : const [];

  /// Convenience: first result set as maps, or empty.
  List<Map<String, dynamic>> toMaps() =>
      results.isNotEmpty ? results.first.toMaps() : const [];
}

/// Error details from the server.
class ErrorDetail {
  final String code;
  final String message;
  final String? details;

  const ErrorDetail({
    required this.code,
    required this.message,
    this.details,
  });

  @override
  String toString() => 'KalamDbError($code): $message';
}

/// Health check response from the server.
class HealthCheckResponse {
  final String status;
  final String version;
  final String apiVersion;
  final String? buildDate;

  const HealthCheckResponse({
    required this.status,
    required this.version,
    required this.apiVersion,
    this.buildDate,
  });
}

/// Login response containing tokens and user info.
class LoginResponse {
  final String accessToken;
  final String? refreshToken;
  final String expiresAt;
  final String? refreshExpiresAt;
  final LoginUserInfo user;

  const LoginResponse({
    required this.accessToken,
    this.refreshToken,
    required this.expiresAt,
    this.refreshExpiresAt,
    required this.user,
  });
}

/// User information returned after login.
class LoginUserInfo {
  final String id;
  final String username;
  final String role;
  final String? email;
  final String createdAt;
  final String updatedAt;

  const LoginUserInfo({
    required this.id,
    required this.username,
    required this.role,
    this.email,
    required this.createdAt,
    required this.updatedAt,
  });
}

/// Server setup request.
class ServerSetupRequest {
  final String username;
  final String password;
  final String rootPassword;
  final String? email;

  const ServerSetupRequest({
    required this.username,
    required this.password,
    required this.rootPassword,
    this.email,
  });
}

/// Server setup response.
class ServerSetupResponse {
  final String message;
  final SetupUserInfo user;

  const ServerSetupResponse({required this.message, required this.user});
}

/// User info from server setup.
class SetupUserInfo {
  final String id;
  final String username;
  final String role;
  final String? email;
  final String createdAt;
  final String updatedAt;

  const SetupUserInfo({
    required this.id,
    required this.username,
    required this.role,
    this.email,
    required this.createdAt,
    required this.updatedAt,
  });
}

/// Setup status response.
class SetupStatusResponse {
  final bool needsSetup;
  final String message;

  const SetupStatusResponse({required this.needsSetup, required this.message});
}

// ---------------------------------------------------------------------------
// Connection lifecycle events / handlers
// ---------------------------------------------------------------------------

/// Reason why a WebSocket connection closed.
class DisconnectReason {
  final String message;
  final int? code;

  const DisconnectReason({required this.message, this.code});
}

/// Error details from a connection or protocol failure.
class ConnectionErrorInfo {
  final String message;
  final bool recoverable;

  const ConnectionErrorInfo({required this.message, required this.recoverable});
}

/// A connection lifecycle event emitted by the KalamDB client.
sealed class ConnectionEvent {
  const ConnectionEvent._();
}

/// Fired when WebSocket connection is established and authenticated.
final class ConnectEvent extends ConnectionEvent {
  const ConnectEvent() : super._();
}

/// Fired when WebSocket connection closes.
final class DisconnectEvent extends ConnectionEvent {
  final DisconnectReason reason;

  const DisconnectEvent({required this.reason}) : super._();
}

/// Fired when a connection/protocol error occurs.
final class ConnectionErrorEvent extends ConnectionEvent {
  final ConnectionErrorInfo error;

  const ConnectionErrorEvent({required this.error}) : super._();
}

/// Debug hook for raw inbound messages.
final class ReceiveEvent extends ConnectionEvent {
  final String message;

  const ReceiveEvent({required this.message}) : super._();
}

/// Debug hook for raw outbound messages.
final class SendEvent extends ConnectionEvent {
  final String message;

  const SendEvent({required this.message}) : super._();
}

/// Optional connection lifecycle callbacks.
class ConnectionHandlers {
  final void Function()? onConnect;
  final void Function(DisconnectReason reason)? onDisconnect;
  final void Function(ConnectionErrorInfo error)? onError;
  final void Function(String message)? onReceive;
  final void Function(String message)? onSend;
  final void Function(ConnectionEvent event)? onEvent;

  const ConnectionHandlers({
    this.onConnect,
    this.onDisconnect,
    this.onError,
    this.onReceive,
    this.onSend,
    this.onEvent,
  });

  bool get hasAny =>
      onConnect != null ||
      onDisconnect != null ||
      onError != null ||
      onReceive != null ||
      onSend != null ||
      onEvent != null;

  /// Dispatch one event to all matching handlers.
  void dispatch(ConnectionEvent event) {
    onEvent?.call(event);
    switch (event) {
      case ConnectEvent():
        onConnect?.call();
      case DisconnectEvent(:final reason):
        onDisconnect?.call(reason);
      case ConnectionErrorEvent(:final error):
        onError?.call(error);
      case ReceiveEvent(:final message):
        onReceive?.call(message);
      case SendEvent(:final message):
        onSend?.call(message);
    }
  }
}

// ---------------------------------------------------------------------------
// Subscription / Change events
// ---------------------------------------------------------------------------

/// A change event from a live subscription.
sealed class ChangeEvent {
  const ChangeEvent._();
}

/// Subscription acknowledged — contains schema metadata.
final class AckEvent extends ChangeEvent {
  final String subscriptionId;
  final int totalRows;
  final List<SchemaField> schema;
  final int batchNum;
  final bool hasMore;
  final String status;

  const AckEvent({
    required this.subscriptionId,
    required this.totalRows,
    required this.schema,
    required this.batchNum,
    required this.hasMore,
    required this.status,
  }) : super._();
}

/// Batch of initial snapshot rows.
final class InitialDataBatch extends ChangeEvent {
  final String subscriptionId;

  /// Raw JSON-encoded row objects.
  final List<String> _rowsJson;
  final int batchNum;
  final bool hasMore;
  final String status;

  InitialDataBatch({
    required this.subscriptionId,
    required List<String> rowsJson,
    required this.batchNum,
    required this.hasMore,
    required this.status,
  })  : _rowsJson = rowsJson,
        super._();

  /// Parsed rows as maps.
  late final List<Map<String, dynamic>> rows = _rowsJson
      .map((json) => Map<String, dynamic>.from(jsonDecode(json) as Map))
      .toList(growable: false);
}

/// One or more rows were inserted.
final class InsertEvent extends ChangeEvent {
  final String subscriptionId;
  final List<String> _rowsJson;

  InsertEvent({
    required this.subscriptionId,
    required List<String> rowsJson,
  })  : _rowsJson = rowsJson,
        super._();

  late final List<Map<String, dynamic>> rows = _rowsJson
      .map((json) => Map<String, dynamic>.from(jsonDecode(json) as Map))
      .toList(growable: false);

  /// Convenience: first inserted row (most common case).
  Map<String, dynamic> get row => rows.first;
}

/// One or more rows were updated.
final class UpdateEvent extends ChangeEvent {
  final String subscriptionId;
  final List<String> _rowsJson;
  final List<String> _oldRowsJson;

  UpdateEvent({
    required this.subscriptionId,
    required List<String> rowsJson,
    required List<String> oldRowsJson,
  })  : _rowsJson = rowsJson,
        _oldRowsJson = oldRowsJson,
        super._();

  late final List<Map<String, dynamic>> rows = _rowsJson
      .map((json) => Map<String, dynamic>.from(jsonDecode(json) as Map))
      .toList(growable: false);

  late final List<Map<String, dynamic>> oldRows = _oldRowsJson
      .map((json) => Map<String, dynamic>.from(jsonDecode(json) as Map))
      .toList(growable: false);

  Map<String, dynamic> get row => rows.first;
  Map<String, dynamic>? get oldRow => oldRows.isNotEmpty ? oldRows.first : null;
}

/// One or more rows were deleted.
final class DeleteEvent extends ChangeEvent {
  final String subscriptionId;
  final List<String> _oldRowsJson;

  DeleteEvent({
    required this.subscriptionId,
    required List<String> oldRowsJson,
  })  : _oldRowsJson = oldRowsJson,
        super._();

  late final List<Map<String, dynamic>> oldRows = _oldRowsJson
      .map((json) => Map<String, dynamic>.from(jsonDecode(json) as Map))
      .toList(growable: false);

  Map<String, dynamic> get row => oldRows.first;
}

/// Server-side error on this subscription.
final class SubscriptionError extends ChangeEvent {
  final String subscriptionId;
  final String code;
  final String message;

  const SubscriptionError({
    required this.subscriptionId,
    required this.code,
    required this.message,
  }) : super._();

  @override
  String toString() => 'SubscriptionError($code): $message';
}
