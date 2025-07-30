# RustMQ Kubernetes Deployment Guide

This guide covers deploying RustMQ in production Kubernetes environments with proper persistent volume management, scaling, and operational features.

## Prerequisites

- Kubernetes cluster (1.20+)
- Storage class supporting persistent volumes (e.g., `fast-ssd`)
- RBAC permissions for StatefulSets and Services

## Quick Start

### 1. Generate Deployment Manifests

Use the built-in Kubernetes deployment manager to generate production-ready manifests:

```rust
use rustmq::operations::kubernetes::{KubernetesDeploymentManager, StatefulSetConfig};
use rustmq::config::KubernetesConfig;

let config = KubernetesConfig {
    use_stateful_sets: true,
    pvc_storage_class: "fast-ssd".to_string(),
    wal_volume_size: "50Gi".to_string(),
    enable_pod_affinity: true,
};

let deployment_manager = KubernetesDeploymentManager::new(config);
let manifest = deployment_manager.generate_complete_manifest(&broker_config_toml);
```

### 2. Core Components

The deployment includes:

- **StatefulSet**: Manages broker pods with persistent storage
- **Headless Service**: Enables broker discovery within the cluster
- **Regular Service**: Provides external access to the cluster
- **ConfigMap**: Centralizes broker configuration
- **PodDisruptionBudget**: Ensures availability during maintenance
- **HorizontalPodAutoscaler**: Automatic scaling based on resource usage

## Configuration Management

### Runtime Configuration Updates

RustMQ supports hot configuration updates without pod restarts:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: rustmq-broker-config
data:
  broker.toml: |
    [wal]
    segment_size_bytes = 134217728  # 128MB
    upload_interval_ms = 600000     # 10 minutes
    flush_interval_ms = 1000        # 1 second
    
    [scaling]
    max_concurrent_additions = 3
    traffic_migration_rate = 0.1
    
    [operations]
    allow_runtime_config_updates = true
    upgrade_velocity = 1
```

### Volume Configuration

Persistent volumes are essential for WAL storage and recovery:

```yaml
volumeClaimTemplates:
- metadata:
    name: wal-storage
  spec:
    accessModes: ["ReadWriteOnce"]
    storageClassName: fast-ssd
    resources:
      requests:
        storage: 50Gi
- metadata:
    name: data-storage
  spec:
    accessModes: ["ReadWriteOnce"] 
    storageClassName: fast-ssd
    resources:
      requests:
        storage: 100Gi
```

## Scaling Operations

### Adding Brokers

Scale up the StatefulSet to add brokers:

```bash
kubectl scale statefulset rustmq-broker --replicas=5
```

RustMQ automatically:
1. Detects new broker pods
2. Performs health verification
3. Rebalances partitions gradually
4. Migrates traffic at configured rate

### Removing Brokers

Scale down safely (one broker at a time):

```bash
kubectl scale statefulset rustmq-broker --replicas=4
```

The system ensures:
1. Graceful shutdown of the highest-numbered pod
2. Partition migration to remaining brokers
3. Data consistency during removal

## Rolling Upgrades

### Automated Rolling Updates

Update the StatefulSet image for zero-downtime upgrades:

```bash
kubectl set image statefulset/rustmq-broker rustmq-broker=rustmq/broker:v2.0.0
```

RustMQ coordinates:
1. One broker upgrade at a time
2. Health verification before proceeding
3. Automatic rollback on failure
4. Progress tracking and status reporting

### Manual Upgrade Control

For more control, use the RustMQ upgrade manager:

```rust
let upgrade_request = UpgradeRequest {
    operation_id: "prod-upgrade-v2.0.0".to_string(),
    target_version: "2.0.0".to_string(),
    upgrade_velocity: 1, // 1 broker per minute
    health_check_timeout: Duration::from_secs(30),
    rollback_on_failure: true,
};

let operation_id = upgrade_manager.start_upgrade(upgrade_request).await?;
```

## Volume Recovery

### Automatic Recovery

RustMQ automatically handles pod failures:

1. **Volume Reattachment**: Persistent volumes automatically reattach to replacement pods
2. **WAL Recovery**: New pods recover from the last uploaded segment
3. **State Reconstruction**: Broker state is rebuilt from persistent storage

### Manual Recovery

For complex recovery scenarios:

```rust
let recovery_manager = VolumeRecoveryManager::new(kubernetes_config);
recovery_manager.recover_broker_with_volume("broker-1", "default").await?;
```

## Monitoring and Observability

### Health Checks

Pods include comprehensive health checks:

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 9094
  initialDelaySeconds: 30
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /ready  
    port: 9094
  initialDelaySeconds: 5
  periodSeconds: 5
```

### Metrics and Monitoring

RustMQ exposes metrics for monitoring:

- WAL segment upload metrics
- Scaling operation progress
- Upgrade status and health
- Volume attachment status

## Production Best Practices

### Resource Allocation

```yaml
resources:
  requests:
    cpu: 1000m      # 1 CPU core
    memory: 4Gi     # 4GB RAM
  limits:
    cpu: 2000m      # 2 CPU cores  
    memory: 8Gi     # 8GB RAM
```

### Storage Classes

Use high-performance storage:

```yaml
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: fast-ssd
provisioner: kubernetes.io/gce-pd
parameters:
  type: pd-ssd
  replication-type: regional-pd
```

### Security Considerations

1. **RBAC**: Limit permissions to necessary operations
2. **Network Policies**: Restrict inter-pod communication
3. **Secret Management**: Use Kubernetes secrets for credentials
4. **Pod Security**: Run with non-root user

### High Availability

1. **Multi-Zone Deployment**: Spread brokers across availability zones
2. **Pod Disruption Budgets**: Maintain minimum available replicas
3. **Resource Quotas**: Prevent resource exhaustion
4. **Backup Strategy**: Regular object storage backups

## Troubleshooting

### Common Issues

1. **Volume Attachment Failures**: Check storage class and node affinity
2. **Upgrade Timeouts**: Verify health check endpoints and timeouts
3. **Scaling Issues**: Monitor partition rebalancing progress
4. **Configuration Errors**: Validate TOML syntax and parameter ranges

### Debug Commands

```bash
# Check broker status
kubectl get pods -l app=rustmq-broker

# View broker logs
kubectl logs rustmq-broker-0

# Check volume attachment
kubectl describe pvc wal-storage-rustmq-broker-0

# Monitor scaling operations
kubectl get events --field-selector involvedObject.name=rustmq-broker
```