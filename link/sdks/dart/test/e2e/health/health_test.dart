/// Health & setup e2e tests — healthCheck, checkSetupStatus.
///
/// Dart-specific: these endpoints exist in the Dart SDK but not in TypeScript.
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:kalam_link/kalam_link.dart';

import '../helpers.dart';

void main() {
  group('Health & Setup', skip: skipIfNoIntegration, () {
    // ─────────────────────────────────────────────────────────────────
    // healthCheck
    // ─────────────────────────────────────────────────────────────────
    test(
      'healthCheck returns server status and version',
      () async {
        await ensureSdkReady();

        final client = await KalamClient.connect(
          url: serverUrl,
          timeout: const Duration(seconds: 10),
        );
        try {
          final health = await client.healthCheck();

          expect(health.status, isNotEmpty);
          expect(health.version, isNotEmpty);
          expect(health.apiVersion, isNotEmpty);
        } finally {
          await client.dispose();
        }
      },
      timeout: const Timeout(Duration(seconds: 30)),
    );

    // ─────────────────────────────────────────────────────────────────
    // checkSetupStatus
    // ─────────────────────────────────────────────────────────────────
    test(
      'checkSetupStatus returns setup state',
      () async {
        await ensureSdkReady();

        final client = await KalamClient.connect(
          url: serverUrl,
          timeout: const Duration(seconds: 10),
        );
        try {
          final status = await client.checkSetupStatus();

          // On a running server that's already set up, needsSetup should be false.
          expect(status.needsSetup, isFalse);
          expect(status.message, isNotEmpty);
        } finally {
          await client.dispose();
        }
      },
      timeout: const Timeout(Duration(seconds: 30)),
    );

    // ─────────────────────────────────────────────────────────────────
    // healthCheck returns valid apiVersion format
    // ─────────────────────────────────────────────────────────────────
    test(
      'healthCheck apiVersion is a non-empty string',
      () async {
        await ensureSdkReady();

        final client = await KalamClient.connect(
          url: serverUrl,
          timeout: const Duration(seconds: 10),
        );
        try {
          final health = await client.healthCheck();
          expect(health.apiVersion.length, greaterThan(0));
        } finally {
          await client.dispose();
        }
      },
      timeout: const Timeout(Duration(seconds: 30)),
    );

    // ─────────────────────────────────────────────────────────────────
    // healthCheck with auth vs. without
    // ─────────────────────────────────────────────────────────────────
    test(
      'healthCheck works without authentication',
      () async {
        await ensureSdkReady();

        // Connect without any auth.
        final client = await KalamClient.connect(
          url: serverUrl,
          timeout: const Duration(seconds: 10),
        );
        try {
          final health = await client.healthCheck();
          expect(health.status, isNotEmpty);
        } finally {
          await client.dispose();
        }
      },
      timeout: const Timeout(Duration(seconds: 30)),
    );

    test(
      'healthCheck works with JWT authentication',
      () async {
        final client = await connectJwtClient();
        try {
          final health = await client.healthCheck();
          expect(health.status, isNotEmpty);
          expect(health.version, isNotEmpty);
        } finally {
          await client.dispose();
        }
      },
      timeout: const Timeout(Duration(seconds: 30)),
    );
  });
}
