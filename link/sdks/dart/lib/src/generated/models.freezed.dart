// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'models.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

T _$identity<T>(T value) => value;

final _privateConstructorUsedError = UnsupportedError(
    'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#adding-getters-and-methods-to-our-models');

/// @nodoc
mixin _$DartAuthProvider {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String username, String password) basicAuth,
    required TResult Function(String token) jwtToken,
    required TResult Function() none,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String username, String password)? basicAuth,
    TResult? Function(String token)? jwtToken,
    TResult? Function()? none,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String username, String password)? basicAuth,
    TResult Function(String token)? jwtToken,
    TResult Function()? none,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartAuthProvider_BasicAuth value) basicAuth,
    required TResult Function(DartAuthProvider_JwtToken value) jwtToken,
    required TResult Function(DartAuthProvider_None value) none,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult? Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult? Function(DartAuthProvider_None value)? none,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult Function(DartAuthProvider_None value)? none,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $DartAuthProviderCopyWith<$Res> {
  factory $DartAuthProviderCopyWith(
          DartAuthProvider value, $Res Function(DartAuthProvider) then) =
      _$DartAuthProviderCopyWithImpl<$Res, DartAuthProvider>;
}

/// @nodoc
class _$DartAuthProviderCopyWithImpl<$Res, $Val extends DartAuthProvider>
    implements $DartAuthProviderCopyWith<$Res> {
  _$DartAuthProviderCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$DartAuthProvider_BasicAuthImplCopyWith<$Res> {
  factory _$$DartAuthProvider_BasicAuthImplCopyWith(
          _$DartAuthProvider_BasicAuthImpl value,
          $Res Function(_$DartAuthProvider_BasicAuthImpl) then) =
      __$$DartAuthProvider_BasicAuthImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String username, String password});
}

/// @nodoc
class __$$DartAuthProvider_BasicAuthImplCopyWithImpl<$Res>
    extends _$DartAuthProviderCopyWithImpl<$Res,
        _$DartAuthProvider_BasicAuthImpl>
    implements _$$DartAuthProvider_BasicAuthImplCopyWith<$Res> {
  __$$DartAuthProvider_BasicAuthImplCopyWithImpl(
      _$DartAuthProvider_BasicAuthImpl _value,
      $Res Function(_$DartAuthProvider_BasicAuthImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? username = null,
    Object? password = null,
  }) {
    return _then(_$DartAuthProvider_BasicAuthImpl(
      username: null == username
          ? _value.username
          : username // ignore: cast_nullable_to_non_nullable
              as String,
      password: null == password
          ? _value.password
          : password // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DartAuthProvider_BasicAuthImpl extends DartAuthProvider_BasicAuth {
  const _$DartAuthProvider_BasicAuthImpl(
      {required this.username, required this.password})
      : super._();

  @override
  final String username;
  @override
  final String password;

  @override
  String toString() {
    return 'DartAuthProvider.basicAuth(username: $username, password: $password)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartAuthProvider_BasicAuthImpl &&
            (identical(other.username, username) ||
                other.username == username) &&
            (identical(other.password, password) ||
                other.password == password));
  }

  @override
  int get hashCode => Object.hash(runtimeType, username, password);

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartAuthProvider_BasicAuthImplCopyWith<_$DartAuthProvider_BasicAuthImpl>
      get copyWith => __$$DartAuthProvider_BasicAuthImplCopyWithImpl<
          _$DartAuthProvider_BasicAuthImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String username, String password) basicAuth,
    required TResult Function(String token) jwtToken,
    required TResult Function() none,
  }) {
    return basicAuth(username, password);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String username, String password)? basicAuth,
    TResult? Function(String token)? jwtToken,
    TResult? Function()? none,
  }) {
    return basicAuth?.call(username, password);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String username, String password)? basicAuth,
    TResult Function(String token)? jwtToken,
    TResult Function()? none,
    required TResult orElse(),
  }) {
    if (basicAuth != null) {
      return basicAuth(username, password);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartAuthProvider_BasicAuth value) basicAuth,
    required TResult Function(DartAuthProvider_JwtToken value) jwtToken,
    required TResult Function(DartAuthProvider_None value) none,
  }) {
    return basicAuth(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult? Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult? Function(DartAuthProvider_None value)? none,
  }) {
    return basicAuth?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult Function(DartAuthProvider_None value)? none,
    required TResult orElse(),
  }) {
    if (basicAuth != null) {
      return basicAuth(this);
    }
    return orElse();
  }
}

abstract class DartAuthProvider_BasicAuth extends DartAuthProvider {
  const factory DartAuthProvider_BasicAuth(
      {required final String username,
      required final String password}) = _$DartAuthProvider_BasicAuthImpl;
  const DartAuthProvider_BasicAuth._() : super._();

  String get username;
  String get password;

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartAuthProvider_BasicAuthImplCopyWith<_$DartAuthProvider_BasicAuthImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DartAuthProvider_JwtTokenImplCopyWith<$Res> {
  factory _$$DartAuthProvider_JwtTokenImplCopyWith(
          _$DartAuthProvider_JwtTokenImpl value,
          $Res Function(_$DartAuthProvider_JwtTokenImpl) then) =
      __$$DartAuthProvider_JwtTokenImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String token});
}

/// @nodoc
class __$$DartAuthProvider_JwtTokenImplCopyWithImpl<$Res>
    extends _$DartAuthProviderCopyWithImpl<$Res,
        _$DartAuthProvider_JwtTokenImpl>
    implements _$$DartAuthProvider_JwtTokenImplCopyWith<$Res> {
  __$$DartAuthProvider_JwtTokenImplCopyWithImpl(
      _$DartAuthProvider_JwtTokenImpl _value,
      $Res Function(_$DartAuthProvider_JwtTokenImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? token = null,
  }) {
    return _then(_$DartAuthProvider_JwtTokenImpl(
      token: null == token
          ? _value.token
          : token // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DartAuthProvider_JwtTokenImpl extends DartAuthProvider_JwtToken {
  const _$DartAuthProvider_JwtTokenImpl({required this.token}) : super._();

  @override
  final String token;

  @override
  String toString() {
    return 'DartAuthProvider.jwtToken(token: $token)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartAuthProvider_JwtTokenImpl &&
            (identical(other.token, token) || other.token == token));
  }

  @override
  int get hashCode => Object.hash(runtimeType, token);

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartAuthProvider_JwtTokenImplCopyWith<_$DartAuthProvider_JwtTokenImpl>
      get copyWith => __$$DartAuthProvider_JwtTokenImplCopyWithImpl<
          _$DartAuthProvider_JwtTokenImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String username, String password) basicAuth,
    required TResult Function(String token) jwtToken,
    required TResult Function() none,
  }) {
    return jwtToken(token);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String username, String password)? basicAuth,
    TResult? Function(String token)? jwtToken,
    TResult? Function()? none,
  }) {
    return jwtToken?.call(token);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String username, String password)? basicAuth,
    TResult Function(String token)? jwtToken,
    TResult Function()? none,
    required TResult orElse(),
  }) {
    if (jwtToken != null) {
      return jwtToken(token);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartAuthProvider_BasicAuth value) basicAuth,
    required TResult Function(DartAuthProvider_JwtToken value) jwtToken,
    required TResult Function(DartAuthProvider_None value) none,
  }) {
    return jwtToken(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult? Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult? Function(DartAuthProvider_None value)? none,
  }) {
    return jwtToken?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult Function(DartAuthProvider_None value)? none,
    required TResult orElse(),
  }) {
    if (jwtToken != null) {
      return jwtToken(this);
    }
    return orElse();
  }
}

abstract class DartAuthProvider_JwtToken extends DartAuthProvider {
  const factory DartAuthProvider_JwtToken({required final String token}) =
      _$DartAuthProvider_JwtTokenImpl;
  const DartAuthProvider_JwtToken._() : super._();

  String get token;

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartAuthProvider_JwtTokenImplCopyWith<_$DartAuthProvider_JwtTokenImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DartAuthProvider_NoneImplCopyWith<$Res> {
  factory _$$DartAuthProvider_NoneImplCopyWith(
          _$DartAuthProvider_NoneImpl value,
          $Res Function(_$DartAuthProvider_NoneImpl) then) =
      __$$DartAuthProvider_NoneImplCopyWithImpl<$Res>;
}

/// @nodoc
class __$$DartAuthProvider_NoneImplCopyWithImpl<$Res>
    extends _$DartAuthProviderCopyWithImpl<$Res, _$DartAuthProvider_NoneImpl>
    implements _$$DartAuthProvider_NoneImplCopyWith<$Res> {
  __$$DartAuthProvider_NoneImplCopyWithImpl(_$DartAuthProvider_NoneImpl _value,
      $Res Function(_$DartAuthProvider_NoneImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartAuthProvider
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc

class _$DartAuthProvider_NoneImpl extends DartAuthProvider_None {
  const _$DartAuthProvider_NoneImpl() : super._();

  @override
  String toString() {
    return 'DartAuthProvider.none()';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartAuthProvider_NoneImpl);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String username, String password) basicAuth,
    required TResult Function(String token) jwtToken,
    required TResult Function() none,
  }) {
    return none();
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String username, String password)? basicAuth,
    TResult? Function(String token)? jwtToken,
    TResult? Function()? none,
  }) {
    return none?.call();
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String username, String password)? basicAuth,
    TResult Function(String token)? jwtToken,
    TResult Function()? none,
    required TResult orElse(),
  }) {
    if (none != null) {
      return none();
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartAuthProvider_BasicAuth value) basicAuth,
    required TResult Function(DartAuthProvider_JwtToken value) jwtToken,
    required TResult Function(DartAuthProvider_None value) none,
  }) {
    return none(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult? Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult? Function(DartAuthProvider_None value)? none,
  }) {
    return none?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartAuthProvider_BasicAuth value)? basicAuth,
    TResult Function(DartAuthProvider_JwtToken value)? jwtToken,
    TResult Function(DartAuthProvider_None value)? none,
    required TResult orElse(),
  }) {
    if (none != null) {
      return none(this);
    }
    return orElse();
  }
}

abstract class DartAuthProvider_None extends DartAuthProvider {
  const factory DartAuthProvider_None() = _$DartAuthProvider_NoneImpl;
  const DartAuthProvider_None._() : super._();
}

/// @nodoc
mixin _$DartChangeEvent {
  String get subscriptionId => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)
        ack,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)
        initialDataBatch,
    required TResult Function(String subscriptionId, List<String> rowsJson)
        insert,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)
        update,
    required TResult Function(String subscriptionId, List<String> oldRowsJson)
        delete,
    required TResult Function(
            String subscriptionId, String code, String message)
        error,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)?
        initialDataBatch,
    TResult? Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult? Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult? Function(String subscriptionId, String code, String message)?
        error,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult Function(String subscriptionId, List<String> rowsJson, int batchNum,
            bool hasMore, String status)?
        initialDataBatch,
    TResult Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult Function(String subscriptionId, String code, String message)? error,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartChangeEvent_Ack value) ack,
    required TResult Function(DartChangeEvent_InitialDataBatch value)
        initialDataBatch,
    required TResult Function(DartChangeEvent_Insert value) insert,
    required TResult Function(DartChangeEvent_Update value) update,
    required TResult Function(DartChangeEvent_Delete value) delete,
    required TResult Function(DartChangeEvent_Error value) error,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartChangeEvent_Ack value)? ack,
    TResult? Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult? Function(DartChangeEvent_Insert value)? insert,
    TResult? Function(DartChangeEvent_Update value)? update,
    TResult? Function(DartChangeEvent_Delete value)? delete,
    TResult? Function(DartChangeEvent_Error value)? error,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartChangeEvent_Ack value)? ack,
    TResult Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult Function(DartChangeEvent_Insert value)? insert,
    TResult Function(DartChangeEvent_Update value)? update,
    TResult Function(DartChangeEvent_Delete value)? delete,
    TResult Function(DartChangeEvent_Error value)? error,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  $DartChangeEventCopyWith<DartChangeEvent> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $DartChangeEventCopyWith<$Res> {
  factory $DartChangeEventCopyWith(
          DartChangeEvent value, $Res Function(DartChangeEvent) then) =
      _$DartChangeEventCopyWithImpl<$Res, DartChangeEvent>;
  @useResult
  $Res call({String subscriptionId});
}

/// @nodoc
class _$DartChangeEventCopyWithImpl<$Res, $Val extends DartChangeEvent>
    implements $DartChangeEventCopyWith<$Res> {
  _$DartChangeEventCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? subscriptionId = null,
  }) {
    return _then(_value.copyWith(
      subscriptionId: null == subscriptionId
          ? _value.subscriptionId
          : subscriptionId // ignore: cast_nullable_to_non_nullable
              as String,
    ) as $Val);
  }
}

/// @nodoc
abstract class _$$DartChangeEvent_AckImplCopyWith<$Res>
    implements $DartChangeEventCopyWith<$Res> {
  factory _$$DartChangeEvent_AckImplCopyWith(_$DartChangeEvent_AckImpl value,
          $Res Function(_$DartChangeEvent_AckImpl) then) =
      __$$DartChangeEvent_AckImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call(
      {String subscriptionId,
      int totalRows,
      List<DartSchemaField> schema,
      int batchNum,
      bool hasMore,
      String status});
}

/// @nodoc
class __$$DartChangeEvent_AckImplCopyWithImpl<$Res>
    extends _$DartChangeEventCopyWithImpl<$Res, _$DartChangeEvent_AckImpl>
    implements _$$DartChangeEvent_AckImplCopyWith<$Res> {
  __$$DartChangeEvent_AckImplCopyWithImpl(_$DartChangeEvent_AckImpl _value,
      $Res Function(_$DartChangeEvent_AckImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? subscriptionId = null,
    Object? totalRows = null,
    Object? schema = null,
    Object? batchNum = null,
    Object? hasMore = null,
    Object? status = null,
  }) {
    return _then(_$DartChangeEvent_AckImpl(
      subscriptionId: null == subscriptionId
          ? _value.subscriptionId
          : subscriptionId // ignore: cast_nullable_to_non_nullable
              as String,
      totalRows: null == totalRows
          ? _value.totalRows
          : totalRows // ignore: cast_nullable_to_non_nullable
              as int,
      schema: null == schema
          ? _value._schema
          : schema // ignore: cast_nullable_to_non_nullable
              as List<DartSchemaField>,
      batchNum: null == batchNum
          ? _value.batchNum
          : batchNum // ignore: cast_nullable_to_non_nullable
              as int,
      hasMore: null == hasMore
          ? _value.hasMore
          : hasMore // ignore: cast_nullable_to_non_nullable
              as bool,
      status: null == status
          ? _value.status
          : status // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DartChangeEvent_AckImpl extends DartChangeEvent_Ack {
  const _$DartChangeEvent_AckImpl(
      {required this.subscriptionId,
      required this.totalRows,
      required final List<DartSchemaField> schema,
      required this.batchNum,
      required this.hasMore,
      required this.status})
      : _schema = schema,
        super._();

  @override
  final String subscriptionId;
  @override
  final int totalRows;
  final List<DartSchemaField> _schema;
  @override
  List<DartSchemaField> get schema {
    if (_schema is EqualUnmodifiableListView) return _schema;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_schema);
  }

  @override
  final int batchNum;
  @override
  final bool hasMore;
  @override
  final String status;

  @override
  String toString() {
    return 'DartChangeEvent.ack(subscriptionId: $subscriptionId, totalRows: $totalRows, schema: $schema, batchNum: $batchNum, hasMore: $hasMore, status: $status)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartChangeEvent_AckImpl &&
            (identical(other.subscriptionId, subscriptionId) ||
                other.subscriptionId == subscriptionId) &&
            (identical(other.totalRows, totalRows) ||
                other.totalRows == totalRows) &&
            const DeepCollectionEquality().equals(other._schema, _schema) &&
            (identical(other.batchNum, batchNum) ||
                other.batchNum == batchNum) &&
            (identical(other.hasMore, hasMore) || other.hasMore == hasMore) &&
            (identical(other.status, status) || other.status == status));
  }

  @override
  int get hashCode => Object.hash(runtimeType, subscriptionId, totalRows,
      const DeepCollectionEquality().hash(_schema), batchNum, hasMore, status);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartChangeEvent_AckImplCopyWith<_$DartChangeEvent_AckImpl> get copyWith =>
      __$$DartChangeEvent_AckImplCopyWithImpl<_$DartChangeEvent_AckImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)
        ack,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)
        initialDataBatch,
    required TResult Function(String subscriptionId, List<String> rowsJson)
        insert,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)
        update,
    required TResult Function(String subscriptionId, List<String> oldRowsJson)
        delete,
    required TResult Function(
            String subscriptionId, String code, String message)
        error,
  }) {
    return ack(subscriptionId, totalRows, schema, batchNum, hasMore, status);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)?
        initialDataBatch,
    TResult? Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult? Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult? Function(String subscriptionId, String code, String message)?
        error,
  }) {
    return ack?.call(
        subscriptionId, totalRows, schema, batchNum, hasMore, status);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult Function(String subscriptionId, List<String> rowsJson, int batchNum,
            bool hasMore, String status)?
        initialDataBatch,
    TResult Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult Function(String subscriptionId, String code, String message)? error,
    required TResult orElse(),
  }) {
    if (ack != null) {
      return ack(subscriptionId, totalRows, schema, batchNum, hasMore, status);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartChangeEvent_Ack value) ack,
    required TResult Function(DartChangeEvent_InitialDataBatch value)
        initialDataBatch,
    required TResult Function(DartChangeEvent_Insert value) insert,
    required TResult Function(DartChangeEvent_Update value) update,
    required TResult Function(DartChangeEvent_Delete value) delete,
    required TResult Function(DartChangeEvent_Error value) error,
  }) {
    return ack(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartChangeEvent_Ack value)? ack,
    TResult? Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult? Function(DartChangeEvent_Insert value)? insert,
    TResult? Function(DartChangeEvent_Update value)? update,
    TResult? Function(DartChangeEvent_Delete value)? delete,
    TResult? Function(DartChangeEvent_Error value)? error,
  }) {
    return ack?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartChangeEvent_Ack value)? ack,
    TResult Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult Function(DartChangeEvent_Insert value)? insert,
    TResult Function(DartChangeEvent_Update value)? update,
    TResult Function(DartChangeEvent_Delete value)? delete,
    TResult Function(DartChangeEvent_Error value)? error,
    required TResult orElse(),
  }) {
    if (ack != null) {
      return ack(this);
    }
    return orElse();
  }
}

abstract class DartChangeEvent_Ack extends DartChangeEvent {
  const factory DartChangeEvent_Ack(
      {required final String subscriptionId,
      required final int totalRows,
      required final List<DartSchemaField> schema,
      required final int batchNum,
      required final bool hasMore,
      required final String status}) = _$DartChangeEvent_AckImpl;
  const DartChangeEvent_Ack._() : super._();

  @override
  String get subscriptionId;
  int get totalRows;
  List<DartSchemaField> get schema;
  int get batchNum;
  bool get hasMore;
  String get status;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartChangeEvent_AckImplCopyWith<_$DartChangeEvent_AckImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DartChangeEvent_InitialDataBatchImplCopyWith<$Res>
    implements $DartChangeEventCopyWith<$Res> {
  factory _$$DartChangeEvent_InitialDataBatchImplCopyWith(
          _$DartChangeEvent_InitialDataBatchImpl value,
          $Res Function(_$DartChangeEvent_InitialDataBatchImpl) then) =
      __$$DartChangeEvent_InitialDataBatchImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call(
      {String subscriptionId,
      List<String> rowsJson,
      int batchNum,
      bool hasMore,
      String status});
}

/// @nodoc
class __$$DartChangeEvent_InitialDataBatchImplCopyWithImpl<$Res>
    extends _$DartChangeEventCopyWithImpl<$Res,
        _$DartChangeEvent_InitialDataBatchImpl>
    implements _$$DartChangeEvent_InitialDataBatchImplCopyWith<$Res> {
  __$$DartChangeEvent_InitialDataBatchImplCopyWithImpl(
      _$DartChangeEvent_InitialDataBatchImpl _value,
      $Res Function(_$DartChangeEvent_InitialDataBatchImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? subscriptionId = null,
    Object? rowsJson = null,
    Object? batchNum = null,
    Object? hasMore = null,
    Object? status = null,
  }) {
    return _then(_$DartChangeEvent_InitialDataBatchImpl(
      subscriptionId: null == subscriptionId
          ? _value.subscriptionId
          : subscriptionId // ignore: cast_nullable_to_non_nullable
              as String,
      rowsJson: null == rowsJson
          ? _value._rowsJson
          : rowsJson // ignore: cast_nullable_to_non_nullable
              as List<String>,
      batchNum: null == batchNum
          ? _value.batchNum
          : batchNum // ignore: cast_nullable_to_non_nullable
              as int,
      hasMore: null == hasMore
          ? _value.hasMore
          : hasMore // ignore: cast_nullable_to_non_nullable
              as bool,
      status: null == status
          ? _value.status
          : status // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DartChangeEvent_InitialDataBatchImpl
    extends DartChangeEvent_InitialDataBatch {
  const _$DartChangeEvent_InitialDataBatchImpl(
      {required this.subscriptionId,
      required final List<String> rowsJson,
      required this.batchNum,
      required this.hasMore,
      required this.status})
      : _rowsJson = rowsJson,
        super._();

  @override
  final String subscriptionId;

  /// Each entry is a JSON-encoded row object (`{"col": value, ...}`).
  final List<String> _rowsJson;

  /// Each entry is a JSON-encoded row object (`{"col": value, ...}`).
  @override
  List<String> get rowsJson {
    if (_rowsJson is EqualUnmodifiableListView) return _rowsJson;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_rowsJson);
  }

  @override
  final int batchNum;
  @override
  final bool hasMore;
  @override
  final String status;

  @override
  String toString() {
    return 'DartChangeEvent.initialDataBatch(subscriptionId: $subscriptionId, rowsJson: $rowsJson, batchNum: $batchNum, hasMore: $hasMore, status: $status)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartChangeEvent_InitialDataBatchImpl &&
            (identical(other.subscriptionId, subscriptionId) ||
                other.subscriptionId == subscriptionId) &&
            const DeepCollectionEquality().equals(other._rowsJson, _rowsJson) &&
            (identical(other.batchNum, batchNum) ||
                other.batchNum == batchNum) &&
            (identical(other.hasMore, hasMore) || other.hasMore == hasMore) &&
            (identical(other.status, status) || other.status == status));
  }

  @override
  int get hashCode => Object.hash(
      runtimeType,
      subscriptionId,
      const DeepCollectionEquality().hash(_rowsJson),
      batchNum,
      hasMore,
      status);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartChangeEvent_InitialDataBatchImplCopyWith<
          _$DartChangeEvent_InitialDataBatchImpl>
      get copyWith => __$$DartChangeEvent_InitialDataBatchImplCopyWithImpl<
          _$DartChangeEvent_InitialDataBatchImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)
        ack,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)
        initialDataBatch,
    required TResult Function(String subscriptionId, List<String> rowsJson)
        insert,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)
        update,
    required TResult Function(String subscriptionId, List<String> oldRowsJson)
        delete,
    required TResult Function(
            String subscriptionId, String code, String message)
        error,
  }) {
    return initialDataBatch(
        subscriptionId, rowsJson, batchNum, hasMore, status);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)?
        initialDataBatch,
    TResult? Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult? Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult? Function(String subscriptionId, String code, String message)?
        error,
  }) {
    return initialDataBatch?.call(
        subscriptionId, rowsJson, batchNum, hasMore, status);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult Function(String subscriptionId, List<String> rowsJson, int batchNum,
            bool hasMore, String status)?
        initialDataBatch,
    TResult Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult Function(String subscriptionId, String code, String message)? error,
    required TResult orElse(),
  }) {
    if (initialDataBatch != null) {
      return initialDataBatch(
          subscriptionId, rowsJson, batchNum, hasMore, status);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartChangeEvent_Ack value) ack,
    required TResult Function(DartChangeEvent_InitialDataBatch value)
        initialDataBatch,
    required TResult Function(DartChangeEvent_Insert value) insert,
    required TResult Function(DartChangeEvent_Update value) update,
    required TResult Function(DartChangeEvent_Delete value) delete,
    required TResult Function(DartChangeEvent_Error value) error,
  }) {
    return initialDataBatch(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartChangeEvent_Ack value)? ack,
    TResult? Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult? Function(DartChangeEvent_Insert value)? insert,
    TResult? Function(DartChangeEvent_Update value)? update,
    TResult? Function(DartChangeEvent_Delete value)? delete,
    TResult? Function(DartChangeEvent_Error value)? error,
  }) {
    return initialDataBatch?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartChangeEvent_Ack value)? ack,
    TResult Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult Function(DartChangeEvent_Insert value)? insert,
    TResult Function(DartChangeEvent_Update value)? update,
    TResult Function(DartChangeEvent_Delete value)? delete,
    TResult Function(DartChangeEvent_Error value)? error,
    required TResult orElse(),
  }) {
    if (initialDataBatch != null) {
      return initialDataBatch(this);
    }
    return orElse();
  }
}

abstract class DartChangeEvent_InitialDataBatch extends DartChangeEvent {
  const factory DartChangeEvent_InitialDataBatch(
      {required final String subscriptionId,
      required final List<String> rowsJson,
      required final int batchNum,
      required final bool hasMore,
      required final String status}) = _$DartChangeEvent_InitialDataBatchImpl;
  const DartChangeEvent_InitialDataBatch._() : super._();

  @override
  String get subscriptionId;

  /// Each entry is a JSON-encoded row object (`{"col": value, ...}`).
  List<String> get rowsJson;
  int get batchNum;
  bool get hasMore;
  String get status;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartChangeEvent_InitialDataBatchImplCopyWith<
          _$DartChangeEvent_InitialDataBatchImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DartChangeEvent_InsertImplCopyWith<$Res>
    implements $DartChangeEventCopyWith<$Res> {
  factory _$$DartChangeEvent_InsertImplCopyWith(
          _$DartChangeEvent_InsertImpl value,
          $Res Function(_$DartChangeEvent_InsertImpl) then) =
      __$$DartChangeEvent_InsertImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({String subscriptionId, List<String> rowsJson});
}

/// @nodoc
class __$$DartChangeEvent_InsertImplCopyWithImpl<$Res>
    extends _$DartChangeEventCopyWithImpl<$Res, _$DartChangeEvent_InsertImpl>
    implements _$$DartChangeEvent_InsertImplCopyWith<$Res> {
  __$$DartChangeEvent_InsertImplCopyWithImpl(
      _$DartChangeEvent_InsertImpl _value,
      $Res Function(_$DartChangeEvent_InsertImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? subscriptionId = null,
    Object? rowsJson = null,
  }) {
    return _then(_$DartChangeEvent_InsertImpl(
      subscriptionId: null == subscriptionId
          ? _value.subscriptionId
          : subscriptionId // ignore: cast_nullable_to_non_nullable
              as String,
      rowsJson: null == rowsJson
          ? _value._rowsJson
          : rowsJson // ignore: cast_nullable_to_non_nullable
              as List<String>,
    ));
  }
}

/// @nodoc

class _$DartChangeEvent_InsertImpl extends DartChangeEvent_Insert {
  const _$DartChangeEvent_InsertImpl(
      {required this.subscriptionId, required final List<String> rowsJson})
      : _rowsJson = rowsJson,
        super._();

  @override
  final String subscriptionId;

  /// Each entry is a JSON-encoded row object.
  final List<String> _rowsJson;

  /// Each entry is a JSON-encoded row object.
  @override
  List<String> get rowsJson {
    if (_rowsJson is EqualUnmodifiableListView) return _rowsJson;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_rowsJson);
  }

  @override
  String toString() {
    return 'DartChangeEvent.insert(subscriptionId: $subscriptionId, rowsJson: $rowsJson)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartChangeEvent_InsertImpl &&
            (identical(other.subscriptionId, subscriptionId) ||
                other.subscriptionId == subscriptionId) &&
            const DeepCollectionEquality().equals(other._rowsJson, _rowsJson));
  }

  @override
  int get hashCode => Object.hash(runtimeType, subscriptionId,
      const DeepCollectionEquality().hash(_rowsJson));

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartChangeEvent_InsertImplCopyWith<_$DartChangeEvent_InsertImpl>
      get copyWith => __$$DartChangeEvent_InsertImplCopyWithImpl<
          _$DartChangeEvent_InsertImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)
        ack,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)
        initialDataBatch,
    required TResult Function(String subscriptionId, List<String> rowsJson)
        insert,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)
        update,
    required TResult Function(String subscriptionId, List<String> oldRowsJson)
        delete,
    required TResult Function(
            String subscriptionId, String code, String message)
        error,
  }) {
    return insert(subscriptionId, rowsJson);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)?
        initialDataBatch,
    TResult? Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult? Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult? Function(String subscriptionId, String code, String message)?
        error,
  }) {
    return insert?.call(subscriptionId, rowsJson);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult Function(String subscriptionId, List<String> rowsJson, int batchNum,
            bool hasMore, String status)?
        initialDataBatch,
    TResult Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult Function(String subscriptionId, String code, String message)? error,
    required TResult orElse(),
  }) {
    if (insert != null) {
      return insert(subscriptionId, rowsJson);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartChangeEvent_Ack value) ack,
    required TResult Function(DartChangeEvent_InitialDataBatch value)
        initialDataBatch,
    required TResult Function(DartChangeEvent_Insert value) insert,
    required TResult Function(DartChangeEvent_Update value) update,
    required TResult Function(DartChangeEvent_Delete value) delete,
    required TResult Function(DartChangeEvent_Error value) error,
  }) {
    return insert(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartChangeEvent_Ack value)? ack,
    TResult? Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult? Function(DartChangeEvent_Insert value)? insert,
    TResult? Function(DartChangeEvent_Update value)? update,
    TResult? Function(DartChangeEvent_Delete value)? delete,
    TResult? Function(DartChangeEvent_Error value)? error,
  }) {
    return insert?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartChangeEvent_Ack value)? ack,
    TResult Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult Function(DartChangeEvent_Insert value)? insert,
    TResult Function(DartChangeEvent_Update value)? update,
    TResult Function(DartChangeEvent_Delete value)? delete,
    TResult Function(DartChangeEvent_Error value)? error,
    required TResult orElse(),
  }) {
    if (insert != null) {
      return insert(this);
    }
    return orElse();
  }
}

abstract class DartChangeEvent_Insert extends DartChangeEvent {
  const factory DartChangeEvent_Insert(
      {required final String subscriptionId,
      required final List<String> rowsJson}) = _$DartChangeEvent_InsertImpl;
  const DartChangeEvent_Insert._() : super._();

  @override
  String get subscriptionId;

  /// Each entry is a JSON-encoded row object.
  List<String> get rowsJson;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartChangeEvent_InsertImplCopyWith<_$DartChangeEvent_InsertImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DartChangeEvent_UpdateImplCopyWith<$Res>
    implements $DartChangeEventCopyWith<$Res> {
  factory _$$DartChangeEvent_UpdateImplCopyWith(
          _$DartChangeEvent_UpdateImpl value,
          $Res Function(_$DartChangeEvent_UpdateImpl) then) =
      __$$DartChangeEvent_UpdateImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call(
      {String subscriptionId, List<String> rowsJson, List<String> oldRowsJson});
}

/// @nodoc
class __$$DartChangeEvent_UpdateImplCopyWithImpl<$Res>
    extends _$DartChangeEventCopyWithImpl<$Res, _$DartChangeEvent_UpdateImpl>
    implements _$$DartChangeEvent_UpdateImplCopyWith<$Res> {
  __$$DartChangeEvent_UpdateImplCopyWithImpl(
      _$DartChangeEvent_UpdateImpl _value,
      $Res Function(_$DartChangeEvent_UpdateImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? subscriptionId = null,
    Object? rowsJson = null,
    Object? oldRowsJson = null,
  }) {
    return _then(_$DartChangeEvent_UpdateImpl(
      subscriptionId: null == subscriptionId
          ? _value.subscriptionId
          : subscriptionId // ignore: cast_nullable_to_non_nullable
              as String,
      rowsJson: null == rowsJson
          ? _value._rowsJson
          : rowsJson // ignore: cast_nullable_to_non_nullable
              as List<String>,
      oldRowsJson: null == oldRowsJson
          ? _value._oldRowsJson
          : oldRowsJson // ignore: cast_nullable_to_non_nullable
              as List<String>,
    ));
  }
}

/// @nodoc

class _$DartChangeEvent_UpdateImpl extends DartChangeEvent_Update {
  const _$DartChangeEvent_UpdateImpl(
      {required this.subscriptionId,
      required final List<String> rowsJson,
      required final List<String> oldRowsJson})
      : _rowsJson = rowsJson,
        _oldRowsJson = oldRowsJson,
        super._();

  @override
  final String subscriptionId;
  final List<String> _rowsJson;
  @override
  List<String> get rowsJson {
    if (_rowsJson is EqualUnmodifiableListView) return _rowsJson;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_rowsJson);
  }

  final List<String> _oldRowsJson;
  @override
  List<String> get oldRowsJson {
    if (_oldRowsJson is EqualUnmodifiableListView) return _oldRowsJson;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_oldRowsJson);
  }

  @override
  String toString() {
    return 'DartChangeEvent.update(subscriptionId: $subscriptionId, rowsJson: $rowsJson, oldRowsJson: $oldRowsJson)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartChangeEvent_UpdateImpl &&
            (identical(other.subscriptionId, subscriptionId) ||
                other.subscriptionId == subscriptionId) &&
            const DeepCollectionEquality().equals(other._rowsJson, _rowsJson) &&
            const DeepCollectionEquality()
                .equals(other._oldRowsJson, _oldRowsJson));
  }

  @override
  int get hashCode => Object.hash(
      runtimeType,
      subscriptionId,
      const DeepCollectionEquality().hash(_rowsJson),
      const DeepCollectionEquality().hash(_oldRowsJson));

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartChangeEvent_UpdateImplCopyWith<_$DartChangeEvent_UpdateImpl>
      get copyWith => __$$DartChangeEvent_UpdateImplCopyWithImpl<
          _$DartChangeEvent_UpdateImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)
        ack,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)
        initialDataBatch,
    required TResult Function(String subscriptionId, List<String> rowsJson)
        insert,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)
        update,
    required TResult Function(String subscriptionId, List<String> oldRowsJson)
        delete,
    required TResult Function(
            String subscriptionId, String code, String message)
        error,
  }) {
    return update(subscriptionId, rowsJson, oldRowsJson);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)?
        initialDataBatch,
    TResult? Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult? Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult? Function(String subscriptionId, String code, String message)?
        error,
  }) {
    return update?.call(subscriptionId, rowsJson, oldRowsJson);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult Function(String subscriptionId, List<String> rowsJson, int batchNum,
            bool hasMore, String status)?
        initialDataBatch,
    TResult Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult Function(String subscriptionId, String code, String message)? error,
    required TResult orElse(),
  }) {
    if (update != null) {
      return update(subscriptionId, rowsJson, oldRowsJson);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartChangeEvent_Ack value) ack,
    required TResult Function(DartChangeEvent_InitialDataBatch value)
        initialDataBatch,
    required TResult Function(DartChangeEvent_Insert value) insert,
    required TResult Function(DartChangeEvent_Update value) update,
    required TResult Function(DartChangeEvent_Delete value) delete,
    required TResult Function(DartChangeEvent_Error value) error,
  }) {
    return update(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartChangeEvent_Ack value)? ack,
    TResult? Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult? Function(DartChangeEvent_Insert value)? insert,
    TResult? Function(DartChangeEvent_Update value)? update,
    TResult? Function(DartChangeEvent_Delete value)? delete,
    TResult? Function(DartChangeEvent_Error value)? error,
  }) {
    return update?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartChangeEvent_Ack value)? ack,
    TResult Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult Function(DartChangeEvent_Insert value)? insert,
    TResult Function(DartChangeEvent_Update value)? update,
    TResult Function(DartChangeEvent_Delete value)? delete,
    TResult Function(DartChangeEvent_Error value)? error,
    required TResult orElse(),
  }) {
    if (update != null) {
      return update(this);
    }
    return orElse();
  }
}

abstract class DartChangeEvent_Update extends DartChangeEvent {
  const factory DartChangeEvent_Update(
      {required final String subscriptionId,
      required final List<String> rowsJson,
      required final List<String> oldRowsJson}) = _$DartChangeEvent_UpdateImpl;
  const DartChangeEvent_Update._() : super._();

  @override
  String get subscriptionId;
  List<String> get rowsJson;
  List<String> get oldRowsJson;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartChangeEvent_UpdateImplCopyWith<_$DartChangeEvent_UpdateImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DartChangeEvent_DeleteImplCopyWith<$Res>
    implements $DartChangeEventCopyWith<$Res> {
  factory _$$DartChangeEvent_DeleteImplCopyWith(
          _$DartChangeEvent_DeleteImpl value,
          $Res Function(_$DartChangeEvent_DeleteImpl) then) =
      __$$DartChangeEvent_DeleteImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({String subscriptionId, List<String> oldRowsJson});
}

/// @nodoc
class __$$DartChangeEvent_DeleteImplCopyWithImpl<$Res>
    extends _$DartChangeEventCopyWithImpl<$Res, _$DartChangeEvent_DeleteImpl>
    implements _$$DartChangeEvent_DeleteImplCopyWith<$Res> {
  __$$DartChangeEvent_DeleteImplCopyWithImpl(
      _$DartChangeEvent_DeleteImpl _value,
      $Res Function(_$DartChangeEvent_DeleteImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? subscriptionId = null,
    Object? oldRowsJson = null,
  }) {
    return _then(_$DartChangeEvent_DeleteImpl(
      subscriptionId: null == subscriptionId
          ? _value.subscriptionId
          : subscriptionId // ignore: cast_nullable_to_non_nullable
              as String,
      oldRowsJson: null == oldRowsJson
          ? _value._oldRowsJson
          : oldRowsJson // ignore: cast_nullable_to_non_nullable
              as List<String>,
    ));
  }
}

/// @nodoc

class _$DartChangeEvent_DeleteImpl extends DartChangeEvent_Delete {
  const _$DartChangeEvent_DeleteImpl(
      {required this.subscriptionId, required final List<String> oldRowsJson})
      : _oldRowsJson = oldRowsJson,
        super._();

  @override
  final String subscriptionId;
  final List<String> _oldRowsJson;
  @override
  List<String> get oldRowsJson {
    if (_oldRowsJson is EqualUnmodifiableListView) return _oldRowsJson;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_oldRowsJson);
  }

  @override
  String toString() {
    return 'DartChangeEvent.delete(subscriptionId: $subscriptionId, oldRowsJson: $oldRowsJson)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartChangeEvent_DeleteImpl &&
            (identical(other.subscriptionId, subscriptionId) ||
                other.subscriptionId == subscriptionId) &&
            const DeepCollectionEquality()
                .equals(other._oldRowsJson, _oldRowsJson));
  }

  @override
  int get hashCode => Object.hash(runtimeType, subscriptionId,
      const DeepCollectionEquality().hash(_oldRowsJson));

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartChangeEvent_DeleteImplCopyWith<_$DartChangeEvent_DeleteImpl>
      get copyWith => __$$DartChangeEvent_DeleteImplCopyWithImpl<
          _$DartChangeEvent_DeleteImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)
        ack,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)
        initialDataBatch,
    required TResult Function(String subscriptionId, List<String> rowsJson)
        insert,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)
        update,
    required TResult Function(String subscriptionId, List<String> oldRowsJson)
        delete,
    required TResult Function(
            String subscriptionId, String code, String message)
        error,
  }) {
    return delete(subscriptionId, oldRowsJson);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)?
        initialDataBatch,
    TResult? Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult? Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult? Function(String subscriptionId, String code, String message)?
        error,
  }) {
    return delete?.call(subscriptionId, oldRowsJson);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult Function(String subscriptionId, List<String> rowsJson, int batchNum,
            bool hasMore, String status)?
        initialDataBatch,
    TResult Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult Function(String subscriptionId, String code, String message)? error,
    required TResult orElse(),
  }) {
    if (delete != null) {
      return delete(subscriptionId, oldRowsJson);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartChangeEvent_Ack value) ack,
    required TResult Function(DartChangeEvent_InitialDataBatch value)
        initialDataBatch,
    required TResult Function(DartChangeEvent_Insert value) insert,
    required TResult Function(DartChangeEvent_Update value) update,
    required TResult Function(DartChangeEvent_Delete value) delete,
    required TResult Function(DartChangeEvent_Error value) error,
  }) {
    return delete(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartChangeEvent_Ack value)? ack,
    TResult? Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult? Function(DartChangeEvent_Insert value)? insert,
    TResult? Function(DartChangeEvent_Update value)? update,
    TResult? Function(DartChangeEvent_Delete value)? delete,
    TResult? Function(DartChangeEvent_Error value)? error,
  }) {
    return delete?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartChangeEvent_Ack value)? ack,
    TResult Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult Function(DartChangeEvent_Insert value)? insert,
    TResult Function(DartChangeEvent_Update value)? update,
    TResult Function(DartChangeEvent_Delete value)? delete,
    TResult Function(DartChangeEvent_Error value)? error,
    required TResult orElse(),
  }) {
    if (delete != null) {
      return delete(this);
    }
    return orElse();
  }
}

abstract class DartChangeEvent_Delete extends DartChangeEvent {
  const factory DartChangeEvent_Delete(
      {required final String subscriptionId,
      required final List<String> oldRowsJson}) = _$DartChangeEvent_DeleteImpl;
  const DartChangeEvent_Delete._() : super._();

  @override
  String get subscriptionId;
  List<String> get oldRowsJson;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartChangeEvent_DeleteImplCopyWith<_$DartChangeEvent_DeleteImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DartChangeEvent_ErrorImplCopyWith<$Res>
    implements $DartChangeEventCopyWith<$Res> {
  factory _$$DartChangeEvent_ErrorImplCopyWith(
          _$DartChangeEvent_ErrorImpl value,
          $Res Function(_$DartChangeEvent_ErrorImpl) then) =
      __$$DartChangeEvent_ErrorImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({String subscriptionId, String code, String message});
}

/// @nodoc
class __$$DartChangeEvent_ErrorImplCopyWithImpl<$Res>
    extends _$DartChangeEventCopyWithImpl<$Res, _$DartChangeEvent_ErrorImpl>
    implements _$$DartChangeEvent_ErrorImplCopyWith<$Res> {
  __$$DartChangeEvent_ErrorImplCopyWithImpl(_$DartChangeEvent_ErrorImpl _value,
      $Res Function(_$DartChangeEvent_ErrorImpl) _then)
      : super(_value, _then);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? subscriptionId = null,
    Object? code = null,
    Object? message = null,
  }) {
    return _then(_$DartChangeEvent_ErrorImpl(
      subscriptionId: null == subscriptionId
          ? _value.subscriptionId
          : subscriptionId // ignore: cast_nullable_to_non_nullable
              as String,
      code: null == code
          ? _value.code
          : code // ignore: cast_nullable_to_non_nullable
              as String,
      message: null == message
          ? _value.message
          : message // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DartChangeEvent_ErrorImpl extends DartChangeEvent_Error {
  const _$DartChangeEvent_ErrorImpl(
      {required this.subscriptionId, required this.code, required this.message})
      : super._();

  @override
  final String subscriptionId;
  @override
  final String code;
  @override
  final String message;

  @override
  String toString() {
    return 'DartChangeEvent.error(subscriptionId: $subscriptionId, code: $code, message: $message)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DartChangeEvent_ErrorImpl &&
            (identical(other.subscriptionId, subscriptionId) ||
                other.subscriptionId == subscriptionId) &&
            (identical(other.code, code) || other.code == code) &&
            (identical(other.message, message) || other.message == message));
  }

  @override
  int get hashCode => Object.hash(runtimeType, subscriptionId, code, message);

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$DartChangeEvent_ErrorImplCopyWith<_$DartChangeEvent_ErrorImpl>
      get copyWith => __$$DartChangeEvent_ErrorImplCopyWithImpl<
          _$DartChangeEvent_ErrorImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)
        ack,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)
        initialDataBatch,
    required TResult Function(String subscriptionId, List<String> rowsJson)
        insert,
    required TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)
        update,
    required TResult Function(String subscriptionId, List<String> oldRowsJson)
        delete,
    required TResult Function(
            String subscriptionId, String code, String message)
        error,
  }) {
    return error(subscriptionId, code, message);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            int batchNum, bool hasMore, String status)?
        initialDataBatch,
    TResult? Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult? Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult? Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult? Function(String subscriptionId, String code, String message)?
        error,
  }) {
    return error?.call(subscriptionId, code, message);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(
            String subscriptionId,
            int totalRows,
            List<DartSchemaField> schema,
            int batchNum,
            bool hasMore,
            String status)?
        ack,
    TResult Function(String subscriptionId, List<String> rowsJson, int batchNum,
            bool hasMore, String status)?
        initialDataBatch,
    TResult Function(String subscriptionId, List<String> rowsJson)? insert,
    TResult Function(String subscriptionId, List<String> rowsJson,
            List<String> oldRowsJson)?
        update,
    TResult Function(String subscriptionId, List<String> oldRowsJson)? delete,
    TResult Function(String subscriptionId, String code, String message)? error,
    required TResult orElse(),
  }) {
    if (error != null) {
      return error(subscriptionId, code, message);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DartChangeEvent_Ack value) ack,
    required TResult Function(DartChangeEvent_InitialDataBatch value)
        initialDataBatch,
    required TResult Function(DartChangeEvent_Insert value) insert,
    required TResult Function(DartChangeEvent_Update value) update,
    required TResult Function(DartChangeEvent_Delete value) delete,
    required TResult Function(DartChangeEvent_Error value) error,
  }) {
    return error(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DartChangeEvent_Ack value)? ack,
    TResult? Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult? Function(DartChangeEvent_Insert value)? insert,
    TResult? Function(DartChangeEvent_Update value)? update,
    TResult? Function(DartChangeEvent_Delete value)? delete,
    TResult? Function(DartChangeEvent_Error value)? error,
  }) {
    return error?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DartChangeEvent_Ack value)? ack,
    TResult Function(DartChangeEvent_InitialDataBatch value)? initialDataBatch,
    TResult Function(DartChangeEvent_Insert value)? insert,
    TResult Function(DartChangeEvent_Update value)? update,
    TResult Function(DartChangeEvent_Delete value)? delete,
    TResult Function(DartChangeEvent_Error value)? error,
    required TResult orElse(),
  }) {
    if (error != null) {
      return error(this);
    }
    return orElse();
  }
}

abstract class DartChangeEvent_Error extends DartChangeEvent {
  const factory DartChangeEvent_Error(
      {required final String subscriptionId,
      required final String code,
      required final String message}) = _$DartChangeEvent_ErrorImpl;
  const DartChangeEvent_Error._() : super._();

  @override
  String get subscriptionId;
  String get code;
  String get message;

  /// Create a copy of DartChangeEvent
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$DartChangeEvent_ErrorImplCopyWith<_$DartChangeEvent_ErrorImpl>
      get copyWith => throw _privateConstructorUsedError;
}
