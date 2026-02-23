import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';

const ROOT = path.resolve(process.cwd());
const packageJsonPath = path.join(ROOT, 'package.json');
const setupSqlPath = path.join(ROOT, 'setup.sql');
const setupShPath = path.join(ROOT, 'setup.sh');
const agentPath = path.join(ROOT, 'src', 'agent.ts');
const runtimePath = path.join(ROOT, 'src', 'summarizer-runtime.ts');

test('package.json uses local sdk and runtime hooks', () => {
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));

  assert.equal(packageJson.scripts.prestart, 'npm run ensure-sdk');
  assert.equal(packageJson.scripts.precheck, 'npm run ensure-sdk');
  assert.match(packageJson.dependencies['kalam-link'], /^file:/);
  assert.ok(packageJson.dependencies['@langchain/google-genai']);
});

test('setup.sql has unquoted topic id and failure table', () => {
  const sql = fs.readFileSync(setupSqlPath, 'utf8');

  assert.match(sql, /CREATE SHARED TABLE IF NOT EXISTS blog\.summary_failures/);
  assert.match(sql, /CREATE TOPIC blog\.summarizer/);
  assert.ok(
    !sql.includes('CREATE TOPIC "blog.summarizer"'),
    'setup.sql should not quote topic id as a single namespace token',
  );
});

test('setup.sh keeps env file by default and supports overwrite flag', () => {
  const script = fs.readFileSync(setupShPath, 'utf8');

  assert.match(script, /--force-env/);
  assert.match(script, /\.env\.local already exists - keeping current file/);
  assert.match(script, /execute_sql_allow_exists/);
  assert.match(script, /already exists\|duplicate\|conflict\|idempotent/);
  assert.match(script, /Verifying failure sink table blog\.summary_failures/);
});

test('agent entrypoint is thin and wires runtime handlers', () => {
  const source = fs.readFileSync(agentPath, 'utf8');

  assert.match(source, /runAgent<.*>\(/s);
  assert.match(source, /createSummarizerHandlers/);
  assert.match(source, /buildGeminiAdapter/);
  assert.match(source, /ackOnFailed:\s*true/);
});

test('runtime module contains llm + row handling logic', () => {
  const source = fs.readFileSync(runtimePath, 'utf8');

  assert.match(source, /createLangChainAdapter/);
  assert.match(source, /ChatGoogleGenerativeAI/);
  assert.match(source, /\[wake\]/);
  assert.ok(!source.includes('ON CONFLICT (run_key)'), 'runtime should avoid unsupported ON CONFLICT upsert');
  assert.match(source, /createSummarizerHandlers/);
});
