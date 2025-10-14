use kalamdb_core::models::Message;
use kalamdb_core::storage::{MessageStore, QueryParams, RocksDbStore};

#[test]
fn test_query_messages_by_conversation_id() {
    let test_dir = format!("./data/test_query_conv_{}", std::process::id());
    let store = RocksDbStore::open(&test_dir).expect("Failed to create store");

    // Insert messages in two different conversations
    let msg1 = Message {
        msg_id: 100,
        conversation_id: "conv_1".to_string(),
        from: "alice".to_string(),
        timestamp: 1000,
        content: "Message 1 in conv_1".to_string(),
        metadata: None,
    };

    let msg2 = Message {
        msg_id: 101,
        conversation_id: "conv_1".to_string(),
        from: "bob".to_string(),
        timestamp: 1001,
        content: "Message 2 in conv_1".to_string(),
        metadata: None,
    };

    let msg3 = Message {
        msg_id: 102,
        conversation_id: "conv_2".to_string(),
        from: "charlie".to_string(),
        timestamp: 1002,
        content: "Message in conv_2".to_string(),
        metadata: None,
    };

    store.insert_message(&msg1).expect("Failed to insert msg1");
    store.insert_message(&msg2).expect("Failed to insert msg2");
    store.insert_message(&msg3).expect("Failed to insert msg3");

    // Query messages from conv_1
    let query = QueryParams {
        conversation_id: Some("conv_1".to_string()),
        since_msg_id: None,
        limit: None,
        order: None,
    };

    let results = store.query_messages(&query).expect("Failed to query messages");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].msg_id, 100);
    assert_eq!(results[0].conversation_id, "conv_1");
    assert_eq!(results[1].msg_id, 101);
    assert_eq!(results[1].conversation_id, "conv_1");

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_query_messages_with_since_msg_id() {
    let test_dir = format!("./data/test_query_since_{}", std::process::id());
    let store = RocksDbStore::open(&test_dir).expect("Failed to create store");

    // Insert sequential messages
    for i in 100..105 {
        let msg = Message {
            msg_id: i,
            conversation_id: "conv_test".to_string(),
            from: "alice".to_string(),
            timestamp: i as i64,
            content: format!("Message {}", i),
            metadata: None,
        };
        store.insert_message(&msg).expect("Failed to insert message");
    }

    // Query messages after msg_id 101
    let query = QueryParams {
        conversation_id: Some("conv_test".to_string()),
        since_msg_id: Some(101),
        limit: None,
        order: None,
    };

    let results = store.query_messages(&query).expect("Failed to query messages");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].msg_id, 102);
    assert_eq!(results[1].msg_id, 103);
    assert_eq!(results[2].msg_id, 104);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_query_messages_with_limit() {
    let test_dir = format!("./data/test_query_limit_{}", std::process::id());
    let store = RocksDbStore::open(&test_dir).expect("Failed to create store");

    // Insert 10 messages
    for i in 100..110 {
        let msg = Message {
            msg_id: i,
            conversation_id: "conv_test".to_string(),
            from: "alice".to_string(),
            timestamp: i as i64,
            content: format!("Message {}", i),
            metadata: None,
        };
        store.insert_message(&msg).expect("Failed to insert message");
    }

    // Query with limit of 5
    let query = QueryParams {
        conversation_id: Some("conv_test".to_string()),
        since_msg_id: None,
        limit: Some(5),
        order: None,
    };

    let results = store.query_messages(&query).expect("Failed to query messages");

    assert_eq!(results.len(), 5);
    assert_eq!(results[0].msg_id, 100);
    assert_eq!(results[4].msg_id, 104);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_query_messages_descending_order() {
    let test_dir = format!("./data/test_query_desc_{}", std::process::id());
    let store = RocksDbStore::open(&test_dir).expect("Failed to create store");

    // Insert messages
    for i in 100..105 {
        let msg = Message {
            msg_id: i,
            conversation_id: "conv_test".to_string(),
            from: "alice".to_string(),
            timestamp: i as i64,
            content: format!("Message {}", i),
            metadata: None,
        };
        store.insert_message(&msg).expect("Failed to insert message");
    }

    // Query in descending order
    let query = QueryParams {
        conversation_id: Some("conv_test".to_string()),
        since_msg_id: None,
        limit: None,
        order: Some("desc".to_string()),
    };

    let results = store.query_messages(&query).expect("Failed to query messages");

    assert_eq!(results.len(), 5);
    assert_eq!(results[0].msg_id, 104);
    assert_eq!(results[1].msg_id, 103);
    assert_eq!(results[2].msg_id, 102);
    assert_eq!(results[3].msg_id, 101);
    assert_eq!(results[4].msg_id, 100);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_query_all_messages_no_filter() {
    let test_dir = format!("./data/test_query_all_{}", std::process::id());
    let store = RocksDbStore::open(&test_dir).expect("Failed to create store");

    // Insert messages in different conversations
    for i in 0..5 {
        let msg = Message {
            msg_id: 100 + i,
            conversation_id: format!("conv_{}", i % 2),
            from: "alice".to_string(),
            timestamp: (100 + i) as i64,
            content: format!("Message {}", i),
            metadata: None,
        };
        store.insert_message(&msg).expect("Failed to insert message");
    }

    // Query all messages (no conversation_id filter)
    let query = QueryParams {
        conversation_id: None,
        since_msg_id: None,
        limit: None,
        order: None,
    };

    let results = store.query_messages(&query).expect("Failed to query messages");

    assert_eq!(results.len(), 5);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_query_messages_empty_result() {
    let test_dir = format!("./data/test_query_empty_{}", std::process::id());
    let store = RocksDbStore::open(&test_dir).expect("Failed to create store");

    // Insert messages in conv_1
    let msg = Message {
        msg_id: 100,
        conversation_id: "conv_1".to_string(),
        from: "alice".to_string(),
        timestamp: 1000,
        content: "Message".to_string(),
        metadata: None,
    };
    store.insert_message(&msg).expect("Failed to insert message");

    // Query non-existent conversation
    let query = QueryParams {
        conversation_id: Some("conv_nonexistent".to_string()),
        since_msg_id: None,
        limit: None,
        order: None,
    };

    let results = store.query_messages(&query).expect("Failed to query messages");

    assert_eq!(results.len(), 0);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_query_messages_combined_filters() {
    let test_dir = format!("./data/test_query_combined_{}", std::process::id());
    let store = RocksDbStore::open(&test_dir).expect("Failed to create store");

    // Insert messages
    for i in 100..110 {
        let msg = Message {
            msg_id: i,
            conversation_id: "conv_test".to_string(),
            from: "alice".to_string(),
            timestamp: i as i64,
            content: format!("Message {}", i),
            metadata: None,
        };
        store.insert_message(&msg).expect("Failed to insert message");
    }

    // Query with conversation_id, since_msg_id, and limit
    let query = QueryParams {
        conversation_id: Some("conv_test".to_string()),
        since_msg_id: Some(102),
        limit: Some(3),
        order: None,
    };

    let results = store.query_messages(&query).expect("Failed to query messages");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].msg_id, 103);
    assert_eq!(results[1].msg_id, 104);
    assert_eq!(results[2].msg_id, 105);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}
