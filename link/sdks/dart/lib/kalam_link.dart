/// KalamDB client SDK for Dart and Flutter.
///
/// Provides query execution, live subscriptions, and authentication
/// for KalamDB servers, powered by flutter_rust_bridge.
///
/// ## Quick Start
///
/// ```dart
/// import 'package:kalam_link/kalam_link.dart';
///
/// final client = await KalamClient.connect(
///   url: 'https://db.example.com',
///   auth: Auth.basic('alice', 'secret123'),
/// );
///
/// final result = await client.query('SELECT * FROM users LIMIT 10');
/// for (final row in result.rows) {
///   print(row);
/// }
///
/// await client.dispose();
/// ```
library;

export 'src/auth.dart';
export 'src/kalam_client.dart';
export 'src/logger.dart';
export 'src/models.dart';
