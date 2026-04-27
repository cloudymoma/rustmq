# RustMQ Backup and Restore Procedure

RustMQ relies on a shared-storage architecture, making backup and restore efficient by focusing on the metadata and object storage.

## 💾 Backup Strategy

### 1. Object Storage (Message Data)
The primary data is stored in Google Cloud Storage.
- **Enable Bucket Versioning**: Already configured in deployment manifests.
- **Bucket Replication**: For multi-region resilience, use GCS Multi-Regional buckets.

### 2. Controller State (Metadata)
The Raft state (topic definitions, ACLs, offsets) is stored on the controller PVCs.
- **Snapshotting**: Controllers automatically create Raft snapshots.
- **Volume Snapshots**: GKE generates snapshots of the `wal-storage` SSDs based on the `StorageClass` policy.

## 🔄 Restore Procedure

### 1. Restoring a Single Node
If a controller or broker pod fails, GKE will restart it.
- **Controller**: Will automatically recover its state from the persistent volume and catch up with the Raft leader.
- **Broker**: Will reload the active WAL from disk and verify its consistency with GCS.

### 2. Full Cluster Recovery
In the event of a total site failure:

1. **Re-deploy Infrastructure**: Use `gke/create-cluster.sh`.
2. **Re-mount GCS Bucket**: Ensure the same GCS bucket is configured in the `ObjectStorageConfig`.
3. **Restore Controller Data**:
   - Recover the latest SSD volume snapshots to new PVCs.
   - Attach the PVCs to the new controller StatefulSet.
4. **Initialize Cluster**: Start the controllers; they will detect the existing state and resume the Raft group.

## 🛡️ Critical Assets
Ensure you have backups of:
- `CLIENT_CERT` / `CLIENT_KEY`: For SDK authentication.
- `RUSTMQ_KEY_ENCRYPTION_PASSWORD`: Required to read any encrypted WAL segments.
