import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalam_link/kalam_link.dart';

void main() {
  // -----------------------------------------------------------------------
  // Auth
  // -----------------------------------------------------------------------
  group('Auth', () {
    test('basic auth stores credentials', () {
      final auth = Auth.basic('alice', 'secret');
      expect(auth, isA<BasicAuth>());
      final basic = auth as BasicAuth;
      expect(basic.username, 'alice');
      expect(basic.password, 'secret');
    });

    test('jwt auth stores token', () {
      final auth = Auth.jwt('eyJ...');
      expect(auth, isA<JwtAuth>());
      expect((auth as JwtAuth).token, 'eyJ...');
    });

    test('none auth', () {
      final auth = Auth.none();
      expect(auth, isA<NoAuth>());
    });
  });

  // -----------------------------------------------------------------------
  // SchemaField
  // -----------------------------------------------------------------------
  group('SchemaField', () {
    test('flag helpers with pk,nn,uq', () {
      const f =
          SchemaField(name: 'id', dataType: 'Int', index: 0, flags: 'pk,nn,uq');
      expect(f.isPrimaryKey, isTrue);
      expect(f.isNonNull, isTrue);
      expect(f.isUnique, isTrue);
    });

    test('flag helpers with pk only', () {
      const f = SchemaField(name: 'id', dataType: 'Int', index: 0, flags: 'pk');
      expect(f.isPrimaryKey, isTrue);
      expect(f.isNonNull, isFalse);
      expect(f.isUnique, isFalse);
    });

    test('flag helpers with null flags', () {
      const f = SchemaField(name: 'name', dataType: 'Text', index: 1);
      expect(f.isPrimaryKey, isFalse);
      expect(f.isNonNull, isFalse);
      expect(f.isUnique, isFalse);
    });
  });

  // -----------------------------------------------------------------------
  // QueryResult
  // -----------------------------------------------------------------------
  group('QueryResult', () {
    test('rows parses JSON arrays', () {
      final qr = QueryResult(
        columns: [
          const SchemaField(name: 'id', dataType: 'Int', index: 0),
          const SchemaField(name: 'name', dataType: 'Text', index: 1),
        ],
        rowsJson: [
          jsonEncode([1, 'Alice']),
          jsonEncode([2, 'Bob']),
        ],
        rowCount: 2,
      );
      expect(qr.rows.length, 2);
      expect(qr.rows[0], [1, 'Alice']);
      expect(qr.rows[1], [2, 'Bob']);
    });

    test('toMaps produces column-keyed maps', () {
      final qr = QueryResult(
        columns: [
          const SchemaField(name: 'id', dataType: 'Int', index: 0),
          const SchemaField(name: 'name', dataType: 'Text', index: 1),
        ],
        rowsJson: [
          jsonEncode([42, 'Charlie'])
        ],
        rowCount: 1,
      );
      final maps = qr.toMaps();
      expect(maps.length, 1);
      expect(maps[0], {'id': 42, 'name': 'Charlie'});
    });

    test('empty rows', () {
      final qr = QueryResult(
        columns: [],
        rowsJson: [],
        rowCount: 0,
        message: 'Table created',
      );
      expect(qr.rows, isEmpty);
      expect(qr.toMaps(), isEmpty);
      expect(qr.message, 'Table created');
    });
  });

  // -----------------------------------------------------------------------
  // QueryResponse convenience accessors
  // -----------------------------------------------------------------------
  group('QueryResponse', () {
    test('convenience getters delegate to first result', () {
      final resp = QueryResponse(
        success: true,
        results: [
          QueryResult(
            columns: [
              const SchemaField(name: 'x', dataType: 'Int', index: 0),
            ],
            rowsJson: [
              jsonEncode([7])
            ],
            rowCount: 1,
          ),
        ],
        tookMs: 3.14,
      );
      expect(resp.success, isTrue);
      expect(resp.columns.length, 1);
      expect(resp.rows, [
        [7]
      ]);
      expect(resp.toMaps(), [
        {'x': 7}
      ]);
      expect(resp.tookMs, closeTo(3.14, 0.001));
    });

    test('convenience getters return empty for no results', () {
      const resp = QueryResponse(success: false, results: []);
      expect(resp.rows, isEmpty);
      expect(resp.columns, isEmpty);
      expect(resp.toMaps(), isEmpty);
    });
  });

  // -----------------------------------------------------------------------
  // ErrorDetail
  // -----------------------------------------------------------------------
  group('ErrorDetail', () {
    test('toString includes code and message', () {
      const e = ErrorDetail(code: 'not_found', message: 'Table missing');
      expect(e.toString(), 'KalamDbError(not_found): Table missing');
    });

    test('details is optional', () {
      const e = ErrorDetail(code: 'err', message: 'msg', details: 'extra');
      expect(e.details, 'extra');
    });
  });

  // -----------------------------------------------------------------------
  // HealthCheckResponse
  // -----------------------------------------------------------------------
  group('HealthCheckResponse', () {
    test('stores all fields', () {
      const h = HealthCheckResponse(
        status: 'healthy',
        version: '0.5.0',
        apiVersion: 'v1',
        buildDate: '2026-02-25',
      );
      expect(h.status, 'healthy');
      expect(h.version, '0.5.0');
      expect(h.apiVersion, 'v1');
      expect(h.buildDate, '2026-02-25');
    });

    test('buildDate is optional', () {
      const h = HealthCheckResponse(
        status: 'ok',
        version: '1.0.0',
        apiVersion: 'v2',
      );
      expect(h.buildDate, isNull);
    });
  });

  // -----------------------------------------------------------------------
  // Login models
  // -----------------------------------------------------------------------
  group('LoginResponse', () {
    test('stores tokens and user info', () {
      const resp = LoginResponse(
        accessToken: 'tok',
        refreshToken: 'ref',
        expiresAt: '2026-12-31',
        refreshExpiresAt: '2027-01-31',
        user: LoginUserInfo(
          id: 'u1',
          username: 'alice',
          role: 'dba',
          email: 'alice@example.com',
          createdAt: '2026-01-01',
          updatedAt: '2026-02-01',
        ),
      );
      expect(resp.accessToken, 'tok');
      expect(resp.refreshToken, 'ref');
      expect(resp.user.username, 'alice');
      expect(resp.user.email, 'alice@example.com');
      expect(resp.user.role, 'dba');
    });
  });

  // -----------------------------------------------------------------------
  // Setup models
  // -----------------------------------------------------------------------
  group('SetupModels', () {
    test('ServerSetupRequest', () {
      const req = ServerSetupRequest(
        username: 'admin',
        password: 'pass',
        rootPassword: 'rootpw',
        email: 'admin@test.com',
      );
      expect(req.username, 'admin');
      expect(req.email, 'admin@test.com');
    });

    test('SetupStatusResponse', () {
      const s = SetupStatusResponse(
        needsSetup: true,
        message: 'Run setup',
      );
      expect(s.needsSetup, isTrue);
      expect(s.message, contains('setup'));
    });
  });

  // -----------------------------------------------------------------------
  // ChangeEvent subclasses
  // -----------------------------------------------------------------------
  group('ChangeEvent', () {
    test('AckEvent stores schema and batch info', () {
      const ack = AckEvent(
        subscriptionId: 'sub-1',
        totalRows: 100,
        schema: [
          SchemaField(name: 'id', dataType: 'Int', index: 0, flags: 'pk')
        ],
        batchNum: 0,
        hasMore: true,
        status: 'loading',
      );
      expect(ack.subscriptionId, 'sub-1');
      expect(ack.totalRows, 100);
      expect(ack.schema.length, 1);
      expect(ack.schema[0].isPrimaryKey, isTrue);
      expect(ack.hasMore, isTrue);
      expect(ack.status, 'loading');
    });

    test('InitialDataBatch lazily parses rows', () {
      final batch = InitialDataBatch(
        subscriptionId: 'sub-2',
        rowsJson: [
          jsonEncode({'id': 1, 'name': 'Alice'}),
          jsonEncode({'id': 2, 'name': 'Bob'}),
        ],
        batchNum: 1,
        hasMore: false,
        status: 'ready',
      );
      expect(batch.rows.length, 2);
      expect(batch.rows[0]['name'], 'Alice');
      expect(batch.rows[1]['id'], 2);
    });

    test('InsertEvent row convenience getter', () {
      final insert = InsertEvent(
        subscriptionId: 'sub-3',
        rowsJson: [
          jsonEncode({'id': 42, 'title': 'Hello'})
        ],
      );
      expect(insert.row['id'], 42);
      expect(insert.row['title'], 'Hello');
    });

    test('UpdateEvent has new and old rows', () {
      final update = UpdateEvent(
        subscriptionId: 'sub-4',
        rowsJson: [
          jsonEncode({'id': 1, 'name': 'Bob2'})
        ],
        oldRowsJson: [
          jsonEncode({'id': 1, 'name': 'Bob'})
        ],
      );
      expect(update.row['name'], 'Bob2');
      expect(update.oldRow?['name'], 'Bob');
    });

    test('DeleteEvent exposes deleted row', () {
      final del = DeleteEvent(
        subscriptionId: 'sub-5',
        oldRowsJson: [
          jsonEncode({'id': 99})
        ],
      );
      expect(del.row['id'], 99);
      expect(del.oldRows.length, 1);
    });

    test('SubscriptionError toString', () {
      const err = SubscriptionError(
        subscriptionId: 'sub-6',
        code: 'auth_fail',
        message: 'Token expired',
      );
      expect(err.toString(), 'SubscriptionError(auth_fail): Token expired');
    });

    test('sealed class exhaustive switch', () {
      ChangeEvent event = const AckEvent(
        subscriptionId: 's',
        totalRows: 0,
        schema: [],
        batchNum: 0,
        hasMore: false,
        status: 'ready',
      );
      // Exhaustive switch — compile-time guarantee all variants handled.
      final label = switch (event) {
        AckEvent() => 'ack',
        InitialDataBatch() => 'batch',
        InsertEvent() => 'insert',
        UpdateEvent() => 'update',
        DeleteEvent() => 'delete',
        SubscriptionError() => 'error',
      };
      expect(label, 'ack');
    });
  });

  // -----------------------------------------------------------------------
  // Connection lifecycle events / handlers
  // -----------------------------------------------------------------------
  group('ConnectionHandlers', () {
    test('hasAny is false when no callbacks are set', () {
      const handlers = ConnectionHandlers();
      expect(handlers.hasAny, isFalse);
    });

    test('hasAny is true when one callback is set', () {
      final handlers = ConnectionHandlers(onConnect: () {});
      expect(handlers.hasAny, isTrue);
    });

    test('dispatch routes each event to matching handler and onEvent', () {
      var connectCount = 0;
      DisconnectReason? disconnectReason;
      ConnectionErrorInfo? errorInfo;
      String? receivedMessage;
      String? sentMessage;
      final seenEvents = <ConnectionEvent>[];

      final handlers = ConnectionHandlers(
        onConnect: () => connectCount++,
        onDisconnect: (reason) => disconnectReason = reason,
        onError: (error) => errorInfo = error,
        onReceive: (message) => receivedMessage = message,
        onSend: (message) => sentMessage = message,
        onEvent: seenEvents.add,
      );

      handlers.dispatch(const ConnectEvent());
      handlers.dispatch(
        const DisconnectEvent(
          reason: DisconnectReason(message: 'closed', code: 1000),
        ),
      );
      handlers.dispatch(
        const ConnectionErrorEvent(
          error: ConnectionErrorInfo(message: 'timeout', recoverable: true),
        ),
      );
      handlers.dispatch(const ReceiveEvent(message: '{"type":"ack"}'));
      handlers.dispatch(const SendEvent(message: '{"type":"ping"}'));

      expect(connectCount, 1);
      expect(disconnectReason?.message, 'closed');
      expect(disconnectReason?.code, 1000);
      expect(errorInfo?.message, 'timeout');
      expect(errorInfo?.recoverable, isTrue);
      expect(receivedMessage, '{"type":"ack"}');
      expect(sentMessage, '{"type":"ping"}');
      expect(seenEvents.length, 5);
      expect(seenEvents[0], isA<ConnectEvent>());
      expect(seenEvents[1], isA<DisconnectEvent>());
      expect(seenEvents[2], isA<ConnectionErrorEvent>());
      expect(seenEvents[3], isA<ReceiveEvent>());
      expect(seenEvents[4], isA<SendEvent>());
    });

    test('sealed ConnectionEvent supports exhaustive switch', () {
      const ConnectionEvent event = SendEvent(message: 'hello');

      final label = switch (event) {
        ConnectEvent() => 'connect',
        DisconnectEvent() => 'disconnect',
        ConnectionErrorEvent() => 'error',
        ReceiveEvent() => 'receive',
        SendEvent() => 'send',
      };

      expect(label, 'send');
    });
  });

  // -----------------------------------------------------------------------
  // SubscriptionInfo
  // -----------------------------------------------------------------------
  group('SubscriptionInfo', () {
    test('all fields populated', () {
      final info = SubscriptionInfo(
        id: 'sub-1',
        query: 'SELECT * FROM users',
        lastSeqId: BigInt.from(42),
        lastEventTimeMs: 1700000000000,
        createdAtMs: 1700000000000,
        closed: false,
      );
      expect(info.id, 'sub-1');
      expect(info.query, 'SELECT * FROM users');
      expect(info.lastSeqId, BigInt.from(42));
      expect(info.lastEventTimeMs, 1700000000000);
      expect(info.createdAtMs, 1700000000000);
      expect(info.closed, isFalse);
    });

    test('optional fields default to null', () {
      const info = SubscriptionInfo(
        id: 'sub-2',
        query: 'SELECT 1',
        createdAtMs: 1700000000000,
        closed: true,
      );
      expect(info.lastSeqId, isNull);
      expect(info.lastEventTimeMs, isNull);
      expect(info.closed, isTrue);
    });

    test('toString includes key fields', () {
      const info = SubscriptionInfo(
        id: 'sub-3',
        query: 'SELECT * FROM t',
        createdAtMs: 1700000000000,
        closed: false,
      );
      final str = info.toString();
      expect(str, contains('sub-3'));
      expect(str, contains('SELECT * FROM t'));
      expect(str, contains('closed: false'));
    });
  });
}
