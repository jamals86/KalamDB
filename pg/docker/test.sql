-- ==========================================================================
-- KalamDB PostgreSQL Extension — Remote Mode End-to-End Test
-- ==========================================================================
-- Tests remote FDW connectivity to a running KalamDB gRPC server.
--
-- Run with:
--   psql -h localhost -p 5433 -U kalamdb -d kalamdb -f test.sql
-- ==========================================================================

\echo '--- Extension info ---'
SELECT pg_kalam_version()       AS version,
       pg_kalam_compiled_mode() AS mode;

-- ==========================================================================
-- 1. SCHEMA OPT-IN
-- ==========================================================================
\echo ''
\echo '=== 0. Verify compiled mode is remote ==='

DO $$
BEGIN
  IF pg_kalam_compiled_mode() <> 'remote' THEN
    RAISE EXCEPTION 'Extension is NOT compiled in remote mode (got: %)', pg_kalam_compiled_mode();
  END IF;
END;
$$;

-- ==========================================================================
-- 1. CREATE SCHEMA + FOREIGN TABLE
-- ==========================================================================
\echo ''
\echo '=== 1. Create schema + foreign table for remote KalamDB table ==='

CREATE SCHEMA IF NOT EXISTS rmtest;
DROP FOREIGN TABLE IF EXISTS rmtest.profiles;

CREATE FOREIGN TABLE rmtest.profiles (
  id TEXT,
  name TEXT,
  age INTEGER,
  _userid TEXT,
  _seq BIGINT,
  _deleted BOOLEAN
) SERVER kalam_server
  OPTIONS (namespace 'rmtest', "table" 'profiles', table_type 'user');

-- Verify the foreign table columns
\echo '--- Foreign table columns ---'
SELECT column_name, data_type
  FROM information_schema.columns
 WHERE table_schema = 'rmtest' AND table_name = 'profiles'
 ORDER BY ordinal_position;

-- ==========================================================================
-- 2. INSERT rows via FDW
-- ==========================================================================
\echo ''
\echo '=== 2. INSERT rows ==='

SET kalam.user_id = 'user-remote-test';

INSERT INTO rmtest.profiles (id, name, age) VALUES ('p1', 'Alice', 30);
INSERT INTO rmtest.profiles (id, name, age) VALUES ('p2', 'Bob', 25);
INSERT INTO rmtest.profiles (id, name, age) VALUES ('p3', 'Charlie', 35);

-- ==========================================================================
-- 3. SELECT rows via FDW
-- ==========================================================================
\echo ''
\echo '=== 3. SELECT rows ==='

SELECT id, name, age FROM rmtest.profiles ORDER BY id;

DO $$
DECLARE
  cnt INTEGER;
BEGIN
  SELECT COUNT(*) INTO cnt FROM rmtest.profiles;
  IF cnt < 3 THEN
    RAISE EXCEPTION 'Expected at least 3 rows, got %', cnt;
  END IF;
END;
$$;

-- ==========================================================================
-- 4. UPDATE row via FDW
-- ==========================================================================
\echo ''
\echo '=== 4. UPDATE row ==='

UPDATE rmtest.profiles SET age = 31 WHERE id = 'p1';
SELECT id, name, age FROM rmtest.profiles WHERE id = 'p1';

DO $$
DECLARE
  v_age INTEGER;
BEGIN
  SELECT age INTO v_age FROM rmtest.profiles WHERE id = 'p1';
  IF v_age <> 31 THEN
    RAISE EXCEPTION 'Expected age=31 after UPDATE, got %', v_age;
  END IF;
END;
$$;

-- ==========================================================================
-- 5. DELETE row via FDW
-- ==========================================================================
\echo ''
\echo '=== 5. DELETE row ==='

DELETE FROM rmtest.profiles WHERE id = 'p3';
SELECT id, name, age FROM rmtest.profiles ORDER BY id;

DO $$
DECLARE
  cnt INTEGER;
BEGIN
  SELECT COUNT(*) INTO cnt FROM rmtest.profiles WHERE id = 'p3';
  IF cnt <> 0 THEN
    RAISE EXCEPTION 'Expected p3 to be deleted, but found % row(s)', cnt;
  END IF;
END;
$$;

-- ==========================================================================
-- 6. CLEANUP
-- ==========================================================================
\echo ''
\echo '=== 6. Cleanup ==='

DELETE FROM rmtest.profiles WHERE id IN ('p1', 'p2');

\echo ''
\echo '========================================'
\echo ' All remote-mode tests passed!'
\echo '========================================'
