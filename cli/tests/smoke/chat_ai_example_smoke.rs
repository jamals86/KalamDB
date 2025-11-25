// Smoke Test: Chat + AI Example from README
// Covers: namespace creation, user table creation, stream table creation,
// insert/query operations, and live subscription to typing events

use crate::common::*;
use std::time::Duration;

#[ntest::timeout(60000)]
#[test]
fn smoke_chat_ai_example_from_readme() {
    if !is_server_running() {
        println!(
            "Skipping smoke_chat_ai_example_from_readme: server not running at {}",
            SERVER_URL
        );
        return;
    }

    // Use unique namespace to avoid collisions
    let namespace = generate_unique_namespace("chat");
    let conversations_table = format!("{}.conversations", namespace);
    let messages_table = format!("{}.messages", namespace);
    let typing_events_table = format!("{}.typing_events", namespace);

    // 1. Create namespace and tables
    let ns_sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace);
    execute_sql_as_root_via_cli(&ns_sql).expect("failed to create namespace");

    let create_conversations = format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            title TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000');",
        conversations_table
    );
    execute_sql_as_root_via_cli(&create_conversations)
        .expect("failed to create conversations table");

    let create_messages = format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            conversation_id BIGINT NOT NULL,
            message_role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000');",
        messages_table
    );
    execute_sql_as_root_via_cli(&create_messages).expect("failed to create messages table");

    let create_typing = format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            conversation_id BIGINT NOT NULL,
            user_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        ) WITH (TYPE = 'STREAM', TTL_SECONDS = 30);",
        typing_events_table
    );
    execute_sql_as_root_via_cli(&create_typing).expect("failed to create typing_events table");

    // 2. Insert a conversation
    let insert_conv_sql = format!(
        "INSERT INTO {} (title) VALUES ('Chat with AI');",
        conversations_table
    );
    execute_sql_as_root_via_cli(&insert_conv_sql).expect("failed to insert conversation");

    // For smoke purposes, we'll use conversation_id = 1
    // (In production, you'd parse the conversation ID from a query)
    let conversation_id = 1;

    // 3. Insert user + AI messages
    let insert_msgs_sql = format!(
        "INSERT INTO {} (conversation_id, message_role, content) VALUES
            ({}, 'user', 'Hello, AI!'),
            ({}, 'assistant', 'Hi! How can I help you today?');",
        messages_table, conversation_id, conversation_id
    );
    execute_sql_as_root_via_cli(&insert_msgs_sql).expect("failed to insert messages");

    // 4. Query back the conversation history
    let select_msgs_sql = format!(
        "SELECT message_role, content FROM {} WHERE conversation_id = {} ORDER BY created_at ASC;",
        messages_table, conversation_id
    );
    let history = execute_sql_as_root_via_cli_json(&select_msgs_sql)
        .expect("failed to query message history");

    assert!(
        history.contains("Hello, AI!"),
        "expected user message in history"
    );
    assert!(
        history.contains("How can I help"),
        "expected assistant message in history"
    );

    // 5. Test stream table with subscription
    let typing_query = format!(
        "SELECT * FROM {} WHERE conversation_id = {}",
        typing_events_table, conversation_id
    );
    let mut listener = SubscriptionListener::start(&typing_query)
        .expect("failed to start subscription for typing events");

    // 6. Insert typing events and verify subscription receives them
    let events = vec![
        ("user_123", "typing"),
        ("ai_model", "thinking"),
        ("ai_model", "cancelled"),
    ];

    for (user_id, event_type) in &events {
        let insert_event_sql = format!(
            "INSERT INTO {} (conversation_id, user_id, event_type) VALUES ({}, '{}', '{}');",
            typing_events_table, conversation_id, user_id, event_type
        );
        execute_sql_as_root_via_cli(&insert_event_sql)
            .expect(&format!("failed to insert typing event: {}", event_type));
    }

    // 7. Wait for subscription to receive at least one event
    let mut received_event = false;
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match listener.try_read_line(Duration::from_millis(500)) {
            Ok(Some(line)) => {
                if !line.trim().is_empty()
                    && (line.contains("typing")
                        || line.contains("thinking")
                        || line.contains("cancelled"))
                {
                    received_event = true;
                    break;
                }
            }
            Ok(None) => break,  // EOF
            Err(_) => continue, // Timeout, keep trying
        }
    }

    assert!(
        received_event,
        "expected to receive at least one typing event via subscription"
    );

    // Stop subscription
    listener.stop().ok();

    // 8. Verify events are queryable via regular SELECT
    let verify_events_sql = format!("SELECT * FROM {}", typing_events_table);
    let events_output = execute_sql_as_root_via_cli_json(&verify_events_sql)
        .expect("failed to query typing events");

    assert!(
        events_output.contains("typing"),
        "expected 'typing' event in stream table"
    );
    assert!(
        events_output.contains("thinking"),
        "expected 'thinking' event in stream table"
    );
}
