<template>
  <div class="brokers">
    <h1>Brokers</h1>

    <div v-if="loading" class="loading">Loading brokers...</div>
    <div v-else-if="error" class="error-message">{{ error }}</div>

    <div v-else>
      <!-- Broker Statistics -->
      <div class="card">
        <h2>Broker Overview</h2>
        <div class="grid grid-cols-4">
          <div class="metric">
            <span class="metric-label">Total Brokers</span>
            <div class="metric-value">{{ brokers.length }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Online</span>
            <div class="metric-value" style="color: #4ade80">{{ onlineCount }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Offline</span>
            <div class="metric-value" style="color: #ef4444">{{ offlineCount }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Health Rate</span>
            <div class="metric-value">{{ healthPercentage }}%</div>
          </div>
        </div>
      </div>

      <!-- Broker List -->
      <div class="card">
        <h2>Broker Details</h2>
        <table>
          <thead>
            <tr>
              <th>Status</th>
              <th>Broker ID</th>
              <th>Host</th>
              <th>QUIC Port</th>
              <th>RPC Port</th>
              <th>Rack ID</th>
              <th>Connections</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="broker in brokers" :key="broker.id">
              <td>
                <span :class="['status-indicator', broker.online ? 'status-online' : 'status-offline']"></span>
                {{ broker.online ? 'Online' : 'Offline' }}
              </td>
              <td><strong>{{ broker.id }}</strong></td>
              <td>{{ broker.host }}</td>
              <td>{{ broker.port_quic }}</td>
              <td>{{ broker.port_rpc }}</td>
              <td>{{ broker.rack_id }}</td>
              <td style="color: #a1a1aa">N/A</td>
            </tr>
          </tbody>
        </table>

        <div style="margin-top: 1rem; padding: 1rem; background-color: #2a2a2a; border-radius: 4px;">
          <p style="color: #a1a1aa; font-size: 0.875rem; margin: 0;">
            💡 <strong>Note:</strong> Detailed broker metrics (throughput, latency, connections) will be available in future updates.
            Currently showing broker registration status and connectivity.
          </p>
        </div>
      </div>

      <!-- Broker Health Check -->
      <div class="card">
        <h2>Health Check</h2>
        <div class="health-grid">
          <div v-for="broker in brokers" :key="broker.id" class="health-card" :class="{ 'health-online': broker.online, 'health-offline': !broker.online }">
            <div class="health-header">
              <span :class="['status-indicator', broker.online ? 'status-online' : 'status-offline']"></span>
              <strong>{{ broker.id }}</strong>
            </div>
            <div class="health-info">
              <div class="health-item">
                <span>Endpoint:</span>
                <span>{{ broker.host }}:{{ broker.port_rpc }}</span>
              </div>
              <div class="health-item">
                <span>Status:</span>
                <span :style="{ color: broker.online ? '#4ade80' : '#ef4444' }">
                  {{ broker.online ? 'Healthy' : 'Unavailable' }}
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import api from '../api/client'
import type { BrokerStatus } from '../api/types'

const loading = ref(true)
const error = ref<string | null>(null)
const brokers = ref<BrokerStatus[]>([])
let refreshInterval: number | null = null

const onlineCount = computed(() => brokers.value.filter(b => b.online).length)
const offlineCount = computed(() => brokers.value.filter(b => !b.online).length)
const healthPercentage = computed(() => {
  if (brokers.value.length === 0) return 0
  return Math.round((onlineCount.value / brokers.value.length) * 100)
})

const fetchBrokers = async () => {
  try {
    error.value = null
    brokers.value = await api.listBrokers()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load brokers'
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await fetchBrokers()
  // Refresh every 5 seconds
  refreshInterval = setInterval(fetchBrokers, 5000) as unknown as number
})

onBeforeUnmount(() => {
  if (refreshInterval !== null) {
    clearInterval(refreshInterval)
  }
})
</script>

<style scoped>
.brokers {
  max-width: 1400px;
}

h1 {
  margin-bottom: 1.5rem;
  font-size: 2rem;
}

h2 {
  margin-bottom: 1rem;
  font-size: 1.25rem;
  color: #e5e5e5;
}

.health-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 1rem;
  margin-top: 1rem;
}

.health-card {
  padding: 1rem;
  border-radius: 8px;
  border: 1px solid #3f3f3f;
  background-color: #2a2a2a;
}

.health-card.health-online {
  border-color: #4ade80;
}

.health-card.health-offline {
  border-color: #ef4444;
}

.health-header {
  display: flex;
  align-items: center;
  margin-bottom: 0.75rem;
  font-size: 1.1rem;
}

.health-info {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.health-item {
  display: flex;
  justify-content: space-between;
  font-size: 0.875rem;
}

.health-item span:first-child {
  color: #a1a1aa;
}
</style>
