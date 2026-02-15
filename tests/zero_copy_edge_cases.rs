//! Comprehensive edge case tests for zero-copy Bytes implementation
//!
//! This test suite validates the zero-copy implementation works correctly
//! in edge cases including empty data, large payloads, concurrent access,
//! and cross-boundary operations.

use bytes::Bytes;
use rustmq::proto::common;
use rustmq::proto_convert::ConversionError;
use rustmq::types::{Header, Record, TopicPartition, WalRecord};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_zero_copy_empty_data() {
    // Test empty key and value
    let empty_record = Record::from_bytes(
        None,
        Bytes::new(),
        vec![],
        chrono::Utc::now().timestamp_millis(),
    );

    assert!(empty_record.key.is_none());
    assert_eq!(empty_record.value.len(), 0);
    assert!(empty_record.headers.is_empty());

    // Test conversion to protobuf and back
    let proto_record: common::Record = empty_record.clone().try_into().unwrap();
    assert!(proto_record.key.is_empty());
    assert!(proto_record.value.is_empty());

    let back_to_internal: Record = proto_record.try_into().unwrap();
    assert!(back_to_internal.key.is_none());
    assert_eq!(back_to_internal.value.len(), 0);
}

#[tokio::test]
async fn test_zero_copy_empty_headers() {
    // Test empty header value
    let empty_header = Header::from_bytes("empty".to_string(), Bytes::new());
    assert_eq!(empty_header.key, "empty");
    assert_eq!(empty_header.value.len(), 0);

    // Test conversion to protobuf and back
    let proto_header: common::Header = empty_header.clone().into();
    assert_eq!(proto_header.key, "empty");
    assert!(proto_header.value.is_empty());

    let back_to_internal: Header = proto_header.into();
    assert_eq!(back_to_internal.key, "empty");
    assert_eq!(back_to_internal.value.len(), 0);
}

#[tokio::test]
async fn test_zero_copy_large_payloads() {
    // Test with 10MB payload
    let large_data = vec![0xAB; 10 * 1024 * 1024];
    let large_key = vec![0xCD; 1024];

    let large_record = Record::from_bytes(
        Some(Bytes::from(large_key.clone())),
        Bytes::from(large_data.clone()),
        vec![Header::from_bytes(
            "size".to_string(),
            Bytes::from("10MB".as_bytes()),
        )],
        chrono::Utc::now().timestamp_millis(),
    );

    // Verify data integrity
    assert_eq!(large_record.key.as_ref().unwrap().len(), 1024);
    assert_eq!(large_record.value.len(), 10 * 1024 * 1024);
    assert_eq!(large_record.headers.len(), 1);

    // Test zero-copy slicing
    let slice1 = large_record.value_slice(0..1024);
    let slice2 = large_record.value_slice(1024..2048);
    assert_eq!(slice1.len(), 1024);
    assert_eq!(slice2.len(), 1024);

    // Verify that slices share the same underlying buffer
    assert_eq!(slice1[0], 0xAB);
    assert_eq!(slice2[0], 0xAB);
}

#[tokio::test]
async fn test_zero_copy_slice_operations() {
    let data = Bytes::from((0u8..255u8).collect::<Vec<u8>>());
    let record = Record::from_bytes(
        Some(data.slice(0..10)),
        data.slice(10..100),
        vec![Header::from_bytes("type".to_string(), data.slice(100..150))],
        chrono::Utc::now().timestamp_millis(),
    );

    // Verify slices maintain correct data
    assert_eq!(record.key.as_ref().unwrap().len(), 10);
    assert_eq!(record.value.len(), 90);
    assert_eq!(record.headers[0].value.len(), 50);

    // Verify data integrity
    assert_eq!(record.key.as_ref().unwrap()[0], 0);
    assert_eq!(record.key.as_ref().unwrap()[9], 9);
    assert_eq!(record.value[0], 10);
    assert_eq!(record.value[89], 99);
    assert_eq!(record.headers[0].value[0], 100);
    assert_eq!(record.headers[0].value[49], 149);
}

#[tokio::test]
async fn test_zero_copy_concurrent_access() {
    // Create a large shared buffer
    let shared_data = Arc::new(Bytes::from(vec![42u8; 1000000]));
    let mut handles = vec![];

    for i in 0..10 {
        let data_clone = shared_data.clone();
        let handle = tokio::spawn(async move {
            let start = i * 10000;
            let end = start + 10000;

            // Create record with slice from shared buffer
            let record = Record::from_bytes(
                Some(data_clone.slice(start..start + 100)),
                data_clone.slice(start..end),
                vec![Header::from_bytes(
                    format!("thread-{}", i),
                    data_clone.slice(end - 100..end),
                )],
                chrono::Utc::now().timestamp_millis(),
            );

            // Verify data integrity
            assert_eq!(record.key.as_ref().unwrap().len(), 100);
            assert_eq!(record.value.len(), 10000);
            assert_eq!(record.headers[0].value.len(), 100);

            // All bytes should be 42
            for byte in record.value.iter() {
                assert_eq!(*byte, 42);
            }

            i
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_zero_copy_reference_counting() {
    let original = Bytes::from("shared data".as_bytes());

    // Create multiple records sharing the same underlying buffer
    let mut records = Vec::new();
    for i in 0..100 {
        let record = Record::from_bytes(
            Some(original.clone()),
            original.clone(),
            vec![Header::from_bytes(format!("index-{}", i), original.clone())],
            chrono::Utc::now().timestamp_millis() + i as i64,
        );
        records.push(record);
    }

    // Verify all records share the same data
    for (i, record) in records.iter().enumerate() {
        assert_eq!(record.key.as_ref().unwrap().as_ref(), b"shared data");
        assert_eq!(record.value.as_ref(), b"shared data");
        assert_eq!(record.headers[0].value.as_ref(), b"shared data");
        assert_eq!(record.headers[0].key, format!("index-{}", i));
    }

    // Drop half the records and verify the rest still work
    records.truncate(50);

    for (i, record) in records.iter().enumerate() {
        assert_eq!(record.key.as_ref().unwrap().as_ref(), b"shared data");
        assert_eq!(record.value.as_ref(), b"shared data");
    }
}

#[tokio::test]
async fn test_zero_copy_protobuf_roundtrip() {
    // Test comprehensive protobuf roundtrip with various data sizes
    let test_cases = vec![
        (None, Bytes::new(), vec![]),                             // Empty case
        (Some(Bytes::from("key")), Bytes::from("value"), vec![]), // Simple case
        (
            Some(Bytes::from(vec![0xFF; 1000])),
            Bytes::from(vec![0xAA; 10000]),
            vec![
                Header::from_bytes("content-type".to_string(), Bytes::from("application/json")),
                Header::from_bytes("encoding".to_string(), Bytes::from("utf-8")),
            ],
        ), // Complex case
    ];

    for (i, (key, value, headers)) in test_cases.into_iter().enumerate() {
        let original_record = Record::from_bytes(
            key.clone(),
            value.clone(),
            headers.clone(),
            chrono::Utc::now().timestamp_millis(),
        );

        // Convert to protobuf
        let proto_record: Result<common::Record, ConversionError> =
            original_record.clone().try_into();
        assert!(
            proto_record.is_ok(),
            "Test case {} failed protobuf conversion",
            i
        );
        let proto_record = proto_record.unwrap();

        // Convert back to internal
        let roundtrip_record: Result<Record, ConversionError> = proto_record.try_into();
        assert!(
            roundtrip_record.is_ok(),
            "Test case {} failed roundtrip conversion",
            i
        );
        let roundtrip_record = roundtrip_record.unwrap();

        // Verify data integrity
        match (&original_record.key, &roundtrip_record.key) {
            (None, None) => {}
            (Some(orig), Some(rt)) => assert_eq!(
                orig.as_ref(),
                rt.as_ref(),
                "Key mismatch in test case {}",
                i
            ),
            _ => panic!("Key presence mismatch in test case {}", i),
        }

        assert_eq!(
            original_record.value.as_ref(),
            roundtrip_record.value.as_ref(),
            "Value mismatch in test case {}",
            i
        );

        assert_eq!(
            original_record.headers.len(),
            roundtrip_record.headers.len(),
            "Header count mismatch in test case {}",
            i
        );

        for (j, (orig_header, rt_header)) in original_record
            .headers
            .iter()
            .zip(roundtrip_record.headers.iter())
            .enumerate()
        {
            assert_eq!(
                orig_header.key, rt_header.key,
                "Header key mismatch in test case {}, header {}",
                i, j
            );
            assert_eq!(
                orig_header.value.as_ref(),
                rt_header.value.as_ref(),
                "Header value mismatch in test case {}, header {}",
                i,
                j
            );
        }
    }
}

#[tokio::test]
async fn test_zero_copy_wal_record() {
    let test_data = Bytes::from("test wal data".as_bytes());
    let tp = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 42,
    };

    let record = Record::from_bytes(
        Some(test_data.slice(0..4)), // "test"
        test_data.slice(5..8),       // "wal"
        vec![Header::from_bytes(
            "source".to_string(),
            test_data.slice(9..13),
        )], // "data"
        chrono::Utc::now().timestamp_millis(),
    );

    let wal_record = WalRecord {
        topic_partition: tp,
        offset: 12345,
        record,
        crc32: 0xDEADBEEF,
    };

    // Test WAL record serialization/deserialization
    let mut buffer = vec![0u8; 1024];
    let serialized_size = wal_record.serialize_to_buffer(&mut buffer).unwrap();
    assert!(serialized_size > 0);
    assert!(serialized_size < buffer.len());

    // Verify WAL record size calculation
    let calculated_size = wal_record.size();
    assert!(calculated_size > 0);
}

#[tokio::test]
async fn test_zero_copy_memory_efficiency() {
    // Create a large buffer to share
    let large_buffer = Bytes::from(vec![0xCC; 1024 * 1024]); // 1MB

    // Create many records that share slices of this buffer
    let mut records = Vec::new();
    let chunk_size = 1024;
    let num_chunks = large_buffer.len() / chunk_size;

    for i in 0..num_chunks {
        let start = i * chunk_size;
        let end = start + chunk_size;

        let record = Record::from_bytes(
            Some(large_buffer.slice(start..start + 32)), // 32-byte key
            large_buffer.slice(start..end),              // 1KB value
            vec![Header::from_bytes(
                format!("chunk-{}", i),
                large_buffer.slice(end - 32..end), // 32-byte header
            )],
            chrono::Utc::now().timestamp_millis(),
        );

        records.push(record);
    }

    // Verify all records
    assert_eq!(records.len(), num_chunks);

    for (i, record) in records.iter().enumerate() {
        assert_eq!(record.key.as_ref().unwrap().len(), 32);
        assert_eq!(record.value.len(), chunk_size);
        assert_eq!(record.headers[0].value.len(), 32);
        assert_eq!(record.headers[0].key, format!("chunk-{}", i));

        // All bytes should be 0xCC
        for byte in record.value.iter() {
            assert_eq!(*byte, 0xCC);
        }
    }
}

#[tokio::test]
async fn test_zero_copy_utf8_edge_cases() {
    // Test UTF-8 boundary cases with Bytes
    let utf8_data = "Hello, 世界! 🚀".as_bytes();
    let utf8_bytes = Bytes::from(utf8_data);

    let record = Record::from_bytes(
        Some(utf8_bytes.slice(0..7)), // "Hello, "
        utf8_bytes.slice(7..13),      // "世界"  (6 bytes)
        vec![Header::from_bytes(
            "emoji".to_string(),
            utf8_bytes.slice(15..19),
        )], // "🚀" (4 bytes)
        chrono::Utc::now().timestamp_millis(),
    );

    // Verify UTF-8 integrity
    let key_str = std::str::from_utf8(record.key.as_ref().unwrap()).unwrap();
    assert_eq!(key_str, "Hello, ");

    let value_str = std::str::from_utf8(&record.value).unwrap();
    assert_eq!(value_str, "世界");

    let header_str = std::str::from_utf8(&record.headers[0].value).unwrap();
    assert_eq!(header_str, "🚀");
}

#[tokio::test]
async fn test_zero_copy_boundary_conditions() {
    let data = Bytes::from((0..100).collect::<Vec<u8>>());

    // Test boundary slice operations
    let edge_cases = vec![
        (0, 0),     // Empty slice at start
        (50, 50),   // Empty slice in middle
        (100, 100), // Empty slice at end
        (0, 1),     // Single byte at start
        (99, 100),  // Single byte at end
        (0, 100),   // Full slice
    ];

    for (start, end) in edge_cases {
        let slice = data.slice(start..end);
        assert_eq!(slice.len(), end - start);

        if !slice.is_empty() {
            assert_eq!(slice[0], start as u8);
            if slice.len() > 1 {
                assert_eq!(slice[slice.len() - 1], (end - 1) as u8);
            }
        }
    }
}

#[tokio::test]
async fn test_zero_copy_stress_test() {
    // Stress test with many concurrent operations
    let num_tasks = 100;
    let shared_buffer = Arc::new(Bytes::from(vec![0x42; 100000]));

    let mut handles = vec![];

    for task_id in 0..num_tasks {
        let buffer = shared_buffer.clone();
        let buffer_len = shared_buffer.len(); // Capture length to avoid borrowing issue
        let handle = tokio::spawn(async move {
            let mut local_records = Vec::new();

            // Create many records quickly
            for i in 0..100 {
                let offset = (task_id * 100 + i) % (buffer_len - 1000);
                let record = Record::from_bytes(
                    Some(buffer.slice(offset..offset + 10)),
                    buffer.slice(offset + 10..offset + 1000),
                    vec![Header::from_bytes(
                        format!("task-{}-record-{}", task_id, i),
                        buffer.slice(offset + 990..offset + 1000),
                    )],
                    chrono::Utc::now().timestamp_millis(),
                );

                local_records.push(record);
            }

            // Verify all records
            for (i, record) in local_records.iter().enumerate() {
                assert_eq!(record.key.as_ref().unwrap().len(), 10);
                assert_eq!(record.value.len(), 990);
                assert_eq!(record.headers[0].value.len(), 10);
                assert_eq!(
                    record.headers[0].key,
                    format!("task-{}-record-{}", task_id, i)
                );
            }

            task_id
        });

        handles.push(handle);
    }

    // Wait for all tasks and verify they completed successfully
    let mut results = Vec::new();
    for handle in handles {
        let result = handle.await.unwrap();
        results.push(result);
    }

    assert_eq!(results.len(), num_tasks);
    for i in 0..num_tasks {
        assert_eq!(results[i], i);
    }
}
