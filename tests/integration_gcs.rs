mod common;

use bytes::Bytes;
use common::gcs::GcsTestConfig;
use object_store::ObjectStore;
use object_store::gcp::{GoogleCloudStorage, GoogleCloudStorageBuilder};
use std::sync::Arc;

#[tokio::test]
#[ignore = "Requires local gcs-test.yaml configuration"]
async fn test_gcs_integration() {
    let config = match GcsTestConfig::load() {
        Some(c) => c,
        None => {
            println!("Skipping GCS integration test: gcs-test.yaml not found");
            return;
        }
    };

    println!(
        "Testing GCS Integration with Project: {}",
        config.project_id
    );

    let mut builder = GoogleCloudStorageBuilder::new().with_bucket_name(&config.bucket_name);

    if let Some(creds) = config.credentials_path.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.with_service_account_path(creds);
    }

    let gcs: GoogleCloudStorage = match builder.build() {
        Ok(s) => s,
        Err(e) => {
            println!("Skipping: Failed to build GCS store: {}", e);
            return;
        }
    };
    let store: Arc<dyn ObjectStore> = Arc::new(gcs);

    let key = object_store::path::Path::from("integration-test/test-file.txt");
    let test_data = Bytes::from("hello world from integration test");

    // Put
    store
        .put(&key, test_data.clone().into())
        .await
        .expect("Failed to put object");

    // Get
    let result = store.get(&key).await.expect("Failed to get object");
    let bytes = result.bytes().await.expect("Failed to get bytes");
    assert_eq!(bytes, test_data);

    // List
    let mut list_stream = store.list(Some(&object_store::path::Path::from("integration-test/")));
    use futures::stream::StreamExt;
    let mut found = false;
    while let Some(meta) = list_stream.next().await {
        let meta = meta.unwrap();
        if meta.location == key {
            found = true;
            break;
        }
    }
    assert!(found, "Object not found in list");

    // Delete
    store.delete(&key).await.expect("Failed to delete object");

    // Verify deletion
    let err = store.head(&key).await.unwrap_err();
    assert!(matches!(err, object_store::Error::NotFound { .. }));
}
