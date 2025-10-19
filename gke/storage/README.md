# RustMQ Storage Architecture

**Phase 3: GKE Infrastructure Optimization** (October 2025)

This directory contains storage configuration for RustMQ GKE deployment.

## Storage Architecture

RustMQ uses a **tiered storage architecture** combining local persistent volumes and cloud object storage:

```
┌─────────────────────────────────────────────────────────────┐
│ RustMQ Broker                                               │
├─────────────────────────────────────────────────────────────┤
│  WAL (Write-Ahead Log)                                      │
│  ↓ (local buffer)                                           │
│  emptyDir (ephemeral)                                       │
│                                                              │
│  Object Storage Client                                      │
│  ↓ (async upload)                                           │
│  Google Cloud Storage (GCS)                                 │
│  - Messages stored as objects                               │
│  - 90% cost reduction vs local storage                      │
│  - Infinite scalability                                     │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ RustMQ Controller                                           │
├─────────────────────────────────────────────────────────────┤
│  Raft WAL (Write-Ahead Log)                                 │
│  ↓ (persistent)                                             │
│  SSD Persistent Disk (pd-ssd)                               │
│  - Low latency for consensus                                │
│  - Regional replication for HA                              │
│  - Retain policy (no data loss)                             │
└─────────────────────────────────────────────────────────────┘
```

## Storage Classes

### rustmq-ssd (Production)

**Purpose**: High-performance SSD storage for controller Raft WAL

**Specifications**:
- **Type**: pd-ssd (Google Cloud SSD Persistent Disk)
- **Replication**: Regional (replicated across zones)
- **Reclaim Policy**: Retain (data preserved even if PVC deleted)
- **Volume Binding**: WaitForFirstConsumer (delayed binding)
- **Expansion**: Supported (can resize without downtime)

**Usage**:
- Controller StatefulSet WAL storage
- Critical consensus data
- Sub-millisecond latency requirements

**Cost**: ~$0.17/GB/month (regional SSD)

### rustmq-standard (Development/Cost-Optimized)

**Purpose**: Standard HDD storage for non-critical workloads

**Specifications**:
- **Type**: pd-standard (Google Cloud Standard Persistent Disk)
- **Replication**: Zonal (single zone)
- **Reclaim Policy**: Delete
- **Volume Binding**: WaitForFirstConsumer
- **Expansion**: Supported

**Usage**:
- Development environments
- Test clusters
- Non-critical data

**Cost**: ~$0.04/GB/month (zonal HDD)

## Persistent Volume Sizing

| Environment | Controllers | Storage per Controller | Total Controller Storage |
|-------------|-------------|------------------------|--------------------------|
| Dev         | 1           | 10 GB                  | 10 GB                    |
| Staging     | 3           | 50 GB                  | 150 GB                   |
| Prod        | 5           | 100 GB                 | 500 GB                   |

**Sizing Guidelines**:
- Raft WAL grows at ~1-5 GB/day depending on cluster activity
- Log compaction runs automatically (configured in controller)
- Snapshots reduce WAL size significantly
- Monitor disk usage and expand if needed

## GCS Object Storage

**Purpose**: Primary message storage backend

**Integration**:
- Brokers write messages to GCS after local WAL buffering
- Workload Identity for authentication (no service account keys)
- Regional buckets for low latency and compliance
- Object versioning for data protection

**Bucket Structure**:
```
gs://rustmq-{environment}-messages/
  topics/
    {topic-name}/
      partitions/
        {partition-id}/
          segments/
            {segment-id}.data
            {segment-id}.index
```

**Cost Optimization**:
- Standard storage class: $0.020/GB/month
- Lifecycle policies: Delete old versions after 90 days
- Compression: Messages compressed before upload
- **90% cost savings** vs. local SSD storage at scale

## Volume Snapshots

**Purpose**: Point-in-time backups of controller WAL

### Manual Snapshot

```bash
# Create snapshot
kubectl exec rustmq-controller-0 -n rustmq -- \
  gcloud compute disks snapshot wal-storage-rustmq-controller-0 \
  --snapshot-names=controller-backup-$(date +%Y%m%d-%H%M%S)

# List snapshots
gcloud compute snapshots list --filter="name:controller-backup"

# Restore from snapshot
kubectl create -f - <<EOF
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: wal-restore
spec:
  dataSource:
    name: controller-backup-20251018-120000
    kind: VolumeSnapshot
  resources:
    requests:
      storage: 100Gi
EOF
```

### Automated Snapshot Schedule

Create a VolumeSnapshotClass and schedule:

```yaml
apiVersion: snapshot.storage.k8s.io/v1
kind: VolumeSnapshotClass
metadata:
  name: rustmq-snapshot
driver: pd.csi.storage.gke.io
deletionPolicy: Retain
```

## Storage Performance

### Controller WAL (SSD)

| Metric              | pd-ssd Regional | Target        |
|---------------------|-----------------|---------------|
| Read IOPS           | 30,000          | > 10,000      |
| Write IOPS          | 30,000          | > 10,000      |
| Read Throughput     | 480 MB/s        | > 100 MB/s    |
| Write Throughput    | 480 MB/s        | > 100 MB/s    |
| Latency (P50)       | < 1 ms          | < 2 ms        |
| Latency (P99)       | < 5 ms          | < 10 ms       |

### GCS Object Storage

| Metric              | Value           | Target        |
|---------------------|-----------------|---------------|
| Write Latency (P50) | 50-100 ms       | < 200 ms      |
| Write Latency (P99) | 200-500 ms      | < 1s          |
| Read Latency (P50)  | 20-50 ms        | < 100 ms      |
| Throughput          | 5 GB/s+         | Unlimited     |

## Monitoring

### Key Metrics to Monitor

**Controller WAL**:
- Disk usage percentage
- IOPS utilization
- Write latency (P50, P99)
- WAL segment count
- Snapshot frequency

**GCS**:
- Upload success rate
- Upload latency
- Storage costs
- API request counts
- Bucket size

### Alerts

```yaml
# Disk usage alert
- alert: ControllerDiskUsageHigh
  expr: kubelet_volume_stats_used_bytes / kubelet_volume_stats_capacity_bytes > 0.8
  for: 10m
  labels:
    severity: warning

# Disk full
- alert: ControllerDiskFull
  expr: kubelet_volume_stats_used_bytes / kubelet_volume_stats_capacity_bytes > 0.95
  for: 5m
  labels:
    severity: critical
```

## Disaster Recovery

### Backup Strategy

1. **Automated VolumeSnapshots**: Daily snapshots of controller WAL
2. **GCS Versioning**: All message data versioned automatically
3. **Cross-Region Replication**: GCS buckets replicated to backup region
4. **Retention Policy**: 30-day snapshot retention

### Recovery Procedures

**Controller Failure**:
1. New pod automatically scheduled (StatefulSet)
2. PVC re-attached to new pod
3. Raft catches up from other controllers
4. Zero data loss (persistent storage)

**Complete Cluster Loss**:
1. Create new GKE cluster
2. Restore controller WAL from snapshots
3. Restore GCS bucket from cross-region replica
4. Brokers reconnect and resume operation
5. RTO: < 1 hour, RPO: < 5 minutes

## Cost Optimization

### Development ($10-20/month)

- 1 controller × 10 GB SSD = $1.70/month
- 1-3 brokers (no persistent storage)
- GCS bucket (< 100 GB) = $2/month
- **Total**: ~$5-10/month storage costs

### Staging ($30-50/month)

- 3 controllers × 50 GB SSD = $25.50/month
- 2-10 brokers (no persistent storage)
- GCS bucket (< 500 GB) = $10/month
- **Total**: ~$35-50/month storage costs

### Production ($100-200/month)

- 5 controllers × 100 GB SSD = $85/month
- 5-50 brokers (no persistent storage)
- GCS bucket (5 TB) = $100/month
- Snapshots = $10-20/month
- **Total**: ~$195-220/month storage costs

### Cost Savings

**vs. Traditional Message Queues** (all data on local SSD):
- 5 TB message data on SSD: $850/month
- 5 TB message data on GCS: $100/month
- **Savings**: $750/month (88% reduction)

## Best Practices

### ✅ DO

- ✅ Use SSD storage for controller WAL (low latency)
- ✅ Use Regional PDs for high availability
- ✅ Enable volume expansion
- ✅ Set reclaim policy to "Retain" for critical data
- ✅ Monitor disk usage and set alerts
- ✅ Schedule regular snapshots
- ✅ Use GCS for message storage (cost-effective)
- ✅ Enable GCS versioning for data protection
- ✅ Implement lifecycle policies to clean up old data

### ❌ DON'T

- ❌ Use HDD for controller WAL (too slow for Raft)
- ❌ Delete PVCs without backups
- ❌ Disable volume expansion
- ❌ Store message data on local disks at scale
- ❌ Skip monitoring disk usage
- ❌ Use "Delete" reclaim policy for critical data

## Troubleshooting

### Disk Full

**Problem**: Controller pod crashing due to full disk

**Diagnosis**:
```bash
kubectl exec rustmq-controller-0 -n rustmq -- df -h /var/lib/rustmq
```

**Solution 1 - Expand Volume**:
```bash
kubectl patch pvc wal-storage-rustmq-controller-0 -n rustmq \
  -p '{"spec":{"resources":{"requests":{"storage":"200Gi"}}}}'
```

**Solution 2 - Trigger Compaction**:
```bash
kubectl exec rustmq-controller-0 -n rustmq -- \
  curl -X POST http://localhost:9095/admin/compact
```

### Slow Write Performance

**Problem**: High write latency on controller WAL

**Diagnosis**:
```bash
# Check IOPS usage
kubectl exec rustmq-controller-0 -n rustmq -- iostat -x 1 5

# Check disk type
kubectl get pv -o yaml | grep type
```

**Solution**:
- Verify using pd-ssd (not pd-standard)
- Check if disk size is sufficient (larger disks = more IOPS)
- Consider increasing disk size for higher IOPS quota

### GCS Upload Failures

**Problem**: Brokers failing to upload messages to GCS

**Diagnosis**:
```bash
# Check broker logs
kubectl logs rustmq-broker-0 -n rustmq | grep -i gcs

# Verify Workload Identity
kubectl describe sa rustmq-sa -n rustmq

# Check GCS permissions
gcloud projects get-iam-policy PROJECT_ID \
  --flatten="bindings[].members" \
  --filter="bindings.members:rustmq-sa"
```

**Solution**:
- Verify Workload Identity annotation on service account
- Check IAM permissions (roles/storage.objectAdmin)
- Verify GCS bucket exists and is accessible

## References

- [GKE Persistent Volumes](https://cloud.google.com/kubernetes-engine/docs/concepts/persistent-volumes)
- [GCE Persistent Disk CSI Driver](https://cloud.google.com/kubernetes-engine/docs/how-to/persistent-volumes/gce-pd-csi-driver)
- [Volume Snapshots](https://cloud.google.com/kubernetes-engine/docs/how-to/persistent-volumes/volume-snapshots)
- [Google Cloud Storage](https://cloud.google.com/storage/docs)
- [Workload Identity](https://cloud.google.com/kubernetes-engine/docs/how-to/workload-identity)

---

**Version**: 1.0
**Last Updated**: October 2025
**Phase**: Phase 3 - GKE Infrastructure Optimization
