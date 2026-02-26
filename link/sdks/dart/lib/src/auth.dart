/// Authentication methods for connecting to KalamDB.
///
/// ```dart
/// // HTTP Basic Auth
/// final auth = Auth.basic('alice', 'secret123');
///
/// // JWT bearer token
/// final auth = Auth.jwt('eyJhbGci...');
///
/// // No authentication (localhost bypass)
/// final auth = Auth.none();
/// ```
sealed class Auth {
  const Auth._();

  /// HTTP Basic Auth with username and password.
  const factory Auth.basic(String username, String password) = BasicAuth;

  /// JWT bearer token authentication.
  const factory Auth.jwt(String token) = JwtAuth;

  /// No authentication (localhost bypass mode).
  const factory Auth.none() = NoAuth;
}

/// HTTP Basic Auth credentials.
final class BasicAuth extends Auth {
  /// The username.
  final String username;

  /// The password.
  final String password;

  const BasicAuth(this.username, this.password) : super._();
}

/// JWT bearer token authentication.
final class JwtAuth extends Auth {
  /// The JWT token string.
  final String token;

  const JwtAuth(this.token) : super._();
}

/// No authentication.
final class NoAuth extends Auth {
  const NoAuth() : super._();
}

/// Async authentication provider callback.
///
/// Called before each (re-)connection attempt to obtain fresh credentials.
/// Ideal for implementing refresh-token flows.
///
/// The returned [Auth] should be [Auth.jwt] or [Auth.none].
/// Returning [Auth.basic] will cause a [StateError] at connect time.
///
/// ```dart
/// Future<KalamClient> create() async {
///   return KalamClient.connect(
///     url: 'https://db.example.com',
///     authProvider: () async {
///       final token = await myApp.getOrRefreshJwt();
///       return Auth.jwt(token);
///     },
///   );
/// }
/// ```
typedef AuthProvider = Future<Auth> Function();
