                "event_id": format!("evt{}", i),
                "value": i * 100
            });
            provider
                .insert_event(&format!("evt{}", i), event)
                .expect("Failed to insert event");
        }

        // Test LIMIT
        let df = ctx
            .sql("SELECT * FROM events LIMIT 3")
            .await
            .expect("Failed to execute SELECT with LIMIT");
        let results = df.collect().await.expect("Failed to collect results");

        let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rows, 3, "Expected 3 rows with LIMIT 3");

        println!("✅ SELECT with LIMIT works");
    }
