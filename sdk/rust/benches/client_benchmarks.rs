use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rustmq_client::*;
use tokio::runtime::Runtime;

fn bench_message_creation(c: &mut Criterion) {
    c.bench_function("message_creation", |b| {
        b.iter(|| {
            let message = Message::builder()
                .topic(black_box("benchmark-topic"))
                .payload(black_box("benchmark payload data"))
                .header("benchmark", "true")
                .build();
            black_box(message)
        })
    });
}

fn bench_message_serialization(c: &mut Criterion) {
    let message = Message::builder()
        .topic("benchmark-topic")
        .payload("benchmark payload data")
        .header("benchmark", "true")
        .build()
        .unwrap();

    c.bench_function("message_serialization", |b| {
        b.iter(|| {
            let json = message.to_json();
            black_box(json)
        })
    });
}

fn bench_message_deserialization(c: &mut Criterion) {
    let message = Message::builder()
        .topic("benchmark-topic")
        .payload("benchmark payload data")
        .header("benchmark", "true")
        .build()
        .unwrap();
    
    let json = message.to_json().unwrap();

    c.bench_function("message_deserialization", |b| {
        b.iter(|| {
            let msg = Message::from_json(black_box(&json));
            black_box(msg)
        })
    });
}

fn bench_batch_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_creation");
    
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("size", size), size, |b, &size| {
            b.iter(|| {
                let messages: Vec<Message> = (0..size)
                    .map(|i| {
                        Message::builder()
                            .topic("benchmark-topic")
                            .payload(format!("message-{}", i))
                            .build()
                            .unwrap()
                    })
                    .collect();
                
                let batch = MessageBatch::new(black_box(messages));
                black_box(batch)
            })
        });
    }
    group.finish();
}

fn bench_config_serialization(c: &mut Criterion) {
    let config = ClientConfig::default();

    c.bench_function("config_serialization", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&config));
            black_box(json)
        })
    });
}

fn bench_config_deserialization(c: &mut Criterion) {
    let config = ClientConfig::default();
    let json = serde_json::to_string(&config).unwrap();

    c.bench_function("config_deserialization", |b| {
        b.iter(|| {
            let config: Result<ClientConfig, _> = serde_json::from_str(black_box(&json));
            black_box(config)
        })
    });
}

fn bench_client_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("client_creation", |b| {
        b.to_async(&rt).iter(|| async {
            let config = ClientConfig {
                brokers: vec!["localhost:9092".to_string()],
                ..Default::default()
            };
            
            // This will fail without a broker, but we're measuring creation overhead
            let result = RustMqClient::new(black_box(config)).await;
            black_box(result)
        })
    });
}

fn bench_message_metadata_extraction(c: &mut Criterion) {
    let message = Message::builder()
        .topic("benchmark-topic")
        .payload("benchmark payload data with some length to make it realistic")
        .header("benchmark", "true")
        .header("timestamp", "1234567890")
        .header("correlation-id", "abc-123-def")
        .build()
        .unwrap();

    c.bench_function("message_metadata_extraction", |b| {
        b.iter(|| {
            let metadata = MessageMetadata::from(black_box(&message));
            black_box(metadata)
        })
    });
}

fn bench_topic_partition_operations(c: &mut Criterion) {
    let tp1 = TopicPartition::new("topic1".to_string(), 0);
    let tp2 = TopicPartition::new("topic1".to_string(), 0);
    let tp3 = TopicPartition::new("topic2".to_string(), 1);

    c.bench_function("topic_partition_equality", |b| {
        b.iter(|| {
            let eq1 = black_box(&tp1) == black_box(&tp2);
            let eq2 = black_box(&tp1) == black_box(&tp3);
            black_box((eq1, eq2))
        })
    });
}

fn bench_error_handling(c: &mut Criterion) {
    c.bench_function("error_categorization", |b| {
        b.iter(|| {
            let errors = vec![
                ClientError::Connection("test".to_string()),
                ClientError::Authentication("test".to_string()),
                ClientError::Timeout { timeout_ms: 1000 },
                ClientError::Producer("test".to_string()),
            ];
            
            let categories: Vec<&str> = errors.iter().map(|e| e.category()).collect();
            let retryable: Vec<bool> = errors.iter().map(|e| e.is_retryable()).collect();
            
            black_box((categories, retryable))
        })
    });
}

criterion_group!(
    benches,
    bench_message_creation,
    bench_message_serialization,
    bench_message_deserialization,
    bench_batch_creation,
    bench_config_serialization,
    bench_config_deserialization,
    bench_client_creation,
    bench_message_metadata_extraction,
    bench_topic_partition_operations,
    bench_error_handling
);

criterion_main!(benches);