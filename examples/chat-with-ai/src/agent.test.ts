import test from 'node:test';
import assert from 'node:assert/strict';
import { buildReply } from './agent.js';

test('buildReply generates a contextual assistant response', () => {
  const reply = buildReply('latency spike');
  assert.ok(reply.includes('AI reply:'), 'should start with AI reply prefix');
  assert.ok(reply.includes('latency spike'), 'should echo the user message');
  assert.ok(reply.includes('slowest route'), 'should include latency-specific advice');
});

test('buildReply handles deploy keyword', () => {
  const reply = buildReply('deploy issue');
  assert.ok(reply.includes('deploy-shaped'), 'should include deploy-specific advice');
});

test('buildReply handles queue keyword', () => {
  const reply = buildReply('queue growth');
  assert.ok(reply.includes('downstream dependency'), 'should include queue-specific advice');
});

test('buildReply uses default advice for generic messages', () => {
  const reply = buildReply('hello world');
  assert.ok(reply.includes('runAgent()'), 'should mention runAgent in default advice');
});