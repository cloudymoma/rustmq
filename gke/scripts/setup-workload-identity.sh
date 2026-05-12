#!/bin/bash
# Setup Workload Identity for RustMQ Brokers to safely access Google Cloud Storage

PROJECT_ID=${1:-"my-gcp-project"}
CLUSTER_NAME=${2:-"rustmq-cluster"}
LOCATION=${3:-"us-central1"}
NAMESPACE="rustmq"
KSA_NAME="rustmq-broker-sa"
GSA_NAME="rustmq-storage-admin"
BUCKET_NAME="rustmq-data"

echo "Setting up Workload Identity on $CLUSTER_NAME..."

# 1. Create a Google Service Account (GSA)
gcloud iam service-accounts create $GSA_NAME \
    --project=$PROJECT_ID \
    --description="Service account for RustMQ broker storage access"

# 2. Grant minimal GCS permissions scoped to the bucket
gcloud storage buckets add-iam-policy-binding gs://$BUCKET_NAME \
    --member="serviceAccount:$GSA_NAME@$PROJECT_ID.iam.gserviceaccount.com" \
    --role="roles/storage.objectAdmin"

# 3. Bind the Kubernetes Service Account (KSA) to the GSA
gcloud iam service-accounts add-iam-policy-binding \
    $GSA_NAME@$PROJECT_ID.iam.gserviceaccount.com \
    --role roles/iam.workloadIdentityUser \
    --member "serviceAccount:$PROJECT_ID.svc.id.goog[$NAMESPACE/$KSA_NAME]"

# 4. Annotate the KSA (Ensure the KSA is deployed in your cluster via deployment manifests)
kubectl annotate serviceaccount $KSA_NAME \
    --namespace $NAMESPACE \
    iam.gke.io/gcp-service-account=$GSA_NAME@$PROJECT_ID.iam.gserviceaccount.com

echo "Workload identity setup complete."
