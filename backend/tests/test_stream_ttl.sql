-- Test stream table TTL eviction and SELECT

-- Create stream table with 2-second TTL
CREATE STREAM TABLE test_events (
    event_id TEXT,
    event_type TEXT,
    timestamp BIGINT
) TTL 2;

-- Insert test events
INSERT INTO test_events VALUES ('evt1', 'click', 1000);
INSERT INTO test_events VALUES ('evt2', 'view', 2000);
INSERT INTO test_events VALUES ('evt3', 'purchase', 3000);

-- Verify events are there
SELECT * FROM test_events;

-- Wait for TTL to expire (need to wait 3+ seconds for eviction to run)
-- Then SELECT again - should be empty or have fewer events
