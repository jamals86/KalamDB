use crate::common;
use crate::common::tcp_proxy::TcpDisconnectProxy;
use super::helpers::*;
use kalam_link::SubscriptionConfig;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Simulate a real network/socket drop by routing the client through a local
/// TCP proxy, force-closing the active socket, then allowing the shared
/// connection to auto-reconnect and resume the same subscription.
#[tokio::test]
async fn test_shared_connection_auto_reconnects_after_socket_drop_and_resumes() {
    let writer = match create_test_client() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping test (writer client unavailable): {}", e);
            return;
        },
    };

    let proxy = TcpDisconnectProxy::start(common::server_url()).await;
    let (client, connect_count, disconnect_count) =
        match create_test_client_with_events_for_base_url(proxy.base_url()) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Skipping test (proxy client unavailable): {}", e);
                proxy.shutdown().await;
                return;
            },
        };

    let suffix = unique_suffix();
    let table = format!("default.socket_drop_resume_{}", suffix);

    ensure_table(&writer, &table).await;

    client
        .connect()
        .await
        .expect("connect should succeed through proxy");

    let mut sub = client
        .subscribe_with_config(SubscriptionConfig::new(
            format!("socket-drop-sub-{}", suffix),
            format!("SELECT id, value FROM {}", table),
        ))
        .await
        .expect("subscribe should succeed through proxy");

    let _ = timeout(TEST_TIMEOUT, sub.next()).await;
    assert!(
        proxy.wait_for_active_connections(1, TEST_TIMEOUT).await,
        "proxy should observe the shared websocket connection"
    );

    let pre_id = "71001";
    let gap_id = "71002";
    let live_id = "71003";

    writer
        .execute_query(
            &format!(
                "INSERT INTO {} (id, value) VALUES ('{}', 'before-drop')",
                table, pre_id
            ),
            None,
            None,
            None,
        )
        .await
        .expect("insert before drop");

    let mut pre_seen = Vec::<String>::new();
    let mut observed_seq = None;
    for _ in 0..12 {
        if pre_seen.iter().any(|id| id == pre_id) {
            break;
        }

        match timeout(Duration::from_millis(1200), sub.next()).await {
            Ok(Some(Ok(ev))) => {
                collect_ids_and_track_seq(
                    &ev,
                    &mut pre_seen,
                    &mut observed_seq,
                    None,
                    "socket drop pre",
                );
            },
            Ok(Some(Err(e))) => panic!("subscription error before socket drop: {}", e),
            Ok(None) => panic!("subscription ended unexpectedly before socket drop"),
            Err(_) => {},
        }
    }

    assert!(
        pre_seen.iter().any(|id| id == pre_id),
        "pre row should be observed before drop"
    );
    let resume_from = query_max_seq(&writer, &table).await;
    let subs = client.subscriptions().await;
    let tracked_seq = subs
        .iter()
        .find(|entry| entry.query == format!("SELECT id, value FROM {}", table))
        .and_then(|entry| entry.last_seq_id);
    assert_eq!(
        tracked_seq,
        Some(resume_from),
        "shared subscription should track last_seq_id before the socket drop"
    );

    let disconnects_before = disconnect_count.load(Ordering::SeqCst);
    proxy.pause();
    proxy.drop_active_connections().await;

    for _ in 0..40 {
        if disconnect_count.load(Ordering::SeqCst) > disconnects_before {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    assert!(
        disconnect_count.load(Ordering::SeqCst) > disconnects_before,
        "forced socket close should trigger an on_disconnect event"
    );

    writer
        .execute_query(
            &format!(
                "INSERT INTO {} (id, value) VALUES ('{}', 'while-disconnected')",
                table, gap_id
            ),
            None,
            None,
            None,
        )
        .await
        .expect("insert while disconnected");

    sleep(Duration::from_millis(300)).await;
    proxy.resume();

    for _ in 0..60 {
        if connect_count.load(Ordering::SeqCst) >= 2 && client.is_connected().await {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    assert!(
        client.is_connected().await,
        "client should auto reconnect after socket drop"
    );
    assert!(
        connect_count.load(Ordering::SeqCst) >= 2,
        "shared connection should emit a second on_connect after reconnect"
    );

    writer
        .execute_query(
            &format!(
                "INSERT INTO {} (id, value) VALUES ('{}', 'after-reconnect')",
                table, live_id
            ),
            None,
            None,
            None,
        )
        .await
        .expect("insert after reconnect");

    let mut resumed_ids = Vec::<String>::new();
    let mut resumed_seq = Some(resume_from);
    for _ in 0..20 {
        if resumed_ids.iter().any(|id| id == gap_id)
            && resumed_ids.iter().any(|id| id == live_id)
        {
            break;
        }

        match timeout(Duration::from_millis(1200), sub.next()).await {
            Ok(Some(Ok(ev))) => {
                collect_ids_and_track_seq(
                    &ev,
                    &mut resumed_ids,
                    &mut resumed_seq,
                    Some(resume_from),
                    "socket drop resumed",
                );
            },
            Ok(Some(Err(e))) => panic!("subscription error after reconnect: {}", e),
            Ok(None) => panic!("subscription ended unexpectedly after reconnect"),
            Err(_) => {},
        }
    }

    assert!(
        resumed_ids.iter().any(|id| id == gap_id),
        "subscription should resume and receive the row written during the disconnect"
    );
    assert!(
        resumed_ids.iter().any(|id| id == live_id),
        "subscription should continue receiving live rows after reconnect"
    );
    assert!(
        !resumed_ids.iter().any(|id| id == pre_id),
        "subscription must not replay rows observed before the socket drop"
    );

    sub.close().await.ok();
    client.disconnect().await;
    proxy.shutdown().await;
}
