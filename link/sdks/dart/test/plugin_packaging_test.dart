import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('declared Flutter plugin platforms have matching directories', () {
    final root = Directory.current;
    final pubspecPath = File('${root.path}/pubspec.yaml');
    final pubspec = pubspecPath.readAsStringSync();

    final pluginDeclared = pubspec.contains('flutter:\n  plugin:');
    final declaredPlatforms = RegExp(r'^ {6}([a-z]+):\s*$', multiLine: true)
        .allMatches(pubspec)
        .map((match) => match.group(1)!)
        .toList(growable: false);

    if (!pluginDeclared) {
      expect(declaredPlatforms, isEmpty);
      return;
    }

    expect(declaredPlatforms, isNotEmpty);
    for (final dirName in declaredPlatforms) {
      final dir = Directory('${root.path}/$dirName');
      expect(
        dir.existsSync(),
        isTrue,
        reason: 'Missing plugin platform directory: $dirName',
      );
    }
  });

  test('android plugin bundles Rust bridge libraries', () {
    final root = Directory.current;
    final pubspecPath = File('${root.path}/pubspec.yaml');
    final pubspec = pubspecPath.readAsStringSync();

    final hasAndroidPlugin = RegExp(
      r'flutter:\s*\n\s*plugin:\s*\n\s*platforms:\s*\n\s*android:\s*\n\s*ffiPlugin:\s*true',
      multiLine: true,
    ).hasMatch(pubspec);

    if (!hasAndroidPlugin) {
      return;
    }

    final requiredAbiDirs = <String>['arm64-v8a', 'x86_64'];
    for (final abi in requiredAbiDirs) {
      final soFile =
          File('${root.path}/android/src/main/jniLibs/$abi/libkalam_link_dart.so');
      expect(
        soFile.existsSync(),
        isTrue,
        reason:
            'Missing Rust bridge library for ABI "$abi": ${soFile.path}',
      );
    }
  });
}
