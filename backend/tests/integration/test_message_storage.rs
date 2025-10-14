// Integration test for message storage
use kalamdb_core::{
    models::Message,
    storage::RocksDbStore,
    ids::SnowflakeGenerator,
};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn test_store_and_retrieve_message() {
    // Setup: Create temporary database
    let temp_dir = TempDir::new().unwrap();
    let db = RocksDbStore::open(temp_dir.path()).unwrap();
    let id_gen = SnowflakeGenerator::new(1);

    // Create a message
    let msg_id = id_gen.next_id().unwrap();
    let message = Message::new(
        msg_id,
        "conv_test_123".to_string(),
        "user_alice".to_string(),
        1699999999000000,
        "Hello, world!".to_string(),
        Some(json!({"role": "user"})),
    );

    // Serialize and store
    let key = format!("msg:{}", msg_id);
    let value = message.to_json_bytes().unwrap();
    db.put(key.as_bytes(), &value).unwrap();

    // Retrieve and verify
    let retrieved = db.get(key.as_bytes()).unwrap().expect("Message not found");
    let retrieved_message = Message::from_json_bytes(&retrieved).unwrap();

    assert_eq!(retrieved_message.msg_id, msg_id);
    assert_eq!(retrieved_message.conversation_id, "conv_test_123");
    assert_eq!(retrieved_message.from, "user_alice");
    assert_eq!(retrieved_message.content, "Hello, world!");
    assert_eq!(retrieved_message.metadata, Some(json!({"role": "user"})));
}

#[test]
fn test_store_multiple_messages() {
    let temp_dir = TempDir::new().unwrap();
    let db = RocksDbStore::open(temp_dir.path()).unwrap();
    let id_gen = SnowflakeGenerator::new(1);

    // Store multiple messages
    let mut msg_ids = Vec::new();
    for i in 0..10 {
        let msg_id = id_gen.next_id().unwrap();
        msg_ids.push(msg_id);

        let message = Message::new(
            msg_id,
            "conv_test_123".to_string(),
            "user_alice".to_string(),
            1699999999000000 + i,
            format!("Message {}", i),
            None,
        );

        let key = format!("msg:{}", msg_id);
        let value = message.to_json_bytes().unwrap();
        db.put(key.as_bytes(), &value).unwrap();
    }

    // Verify all messages can be retrieved
    for msg_id in msg_ids {
        let key = format!("msg:{}", msg_id);
        let retrieved = db.get(key.as_bytes()).unwrap();
        assert!(retrieved.is_some(), "Message {} not found", msg_id);
    }
}

#[test]
fn test_message_persistence_across_reopens() {
    let temp_dir = TempDir::new().unwrap();
    let msg_id = 123456789i64;
    let key = format!("msg:{}", msg_id);

    // Store message in first db instance
    {
        let db = RocksDbStore::open(temp_dir.path()).unwrap();
        let message = Message::new(
            msg_id,
            "conv_test_123".to_string(),
            "user_alice".to_string(),
            1699999999000000,
            "Persistent message".to_string(),
            None,
        );
        let value = message.to_json_bytes().unwrap();
        db.put(key.as_bytes(), &value).unwrap();
        db.flush().unwrap();
    } // db is dropped here

    // Retrieve message in second db instance
    {
        let db = RocksDbStore::open(temp_dir.path()).unwrap();
        let retrieved = db.get(key.as_bytes()).unwrap().expect("Message not found after reopen");
        let message = Message::from_json_bytes(&retrieved).unwrap();
        assert_eq!(message.content, "Persistent message");
    }
}

#[test]
fn test_query_messages_by_prefix() {
    let temp_dir = TempDir::new().unwrap();
    let db = RocksDbStore::open(temp_dir.path()).unwrap();
    let id_gen = SnowflakeGenerator::new(1);

    // Store messages with different prefixes
    for i in 0..5 {
        let msg_id = id_gen.next_id().unwrap();
        let message = Message::new(
            msg_id,
            "conv_123".to_string(),
            "user_alice".to_string(),
            1699999999000000 + i,
            format!("Message {}", i),
            None,
        );
        let key = format!("msg:{}", msg_id);
        db.put(key.as_bytes(), &message.to_json_bytes().unwrap()).unwrap();
    }

    for i in 0..3 {
        let msg_id = id_gen.next_id().unwrap();
        let message = Message::new(
            msg_id,
            "conv_456".to_string(),
            "user_bob".to_string(),
            1699999999000000 + i,
            format!("Message {}", i),
            None,
        );
        let key = format!("msg:{}", msg_id);
        db.put(key.as_bytes(), &message.to_json_bytes().unwrap()).unwrap();
    }

    // Query all messages
    let all_messages: Vec<_> = db.iter_prefix(b"msg:").collect();
    assert_eq!(all_messages.len(), 8);
}

#[test]
fn test_message_validation_before_storage() {
    let message = Message::new(
        123456789,
        "conv_test".to_string(),
        "user_alice".to_string(),
        1699999999000000,
        "Valid message".to_string(),
        None,
    );

    // Valid message should pass
    assert!(message.validate(1048576).is_ok());

    // Message too large should fail
    let large_message = Message::new(
        123456789,
        "conv_test".to_string(),
        "user_alice".to_string(),
        1699999999000000,
        "x".repeat(2_000_000),
        None,
    );
    assert!(large_message.validate(1048576).is_err());
}
