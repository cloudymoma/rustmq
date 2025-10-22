import axios, { AxiosInstance } from 'axios'
import type {
  ApiResponse,
  HealthResponse,
  ClusterStatus,
  TopicSummary,
  TopicDetail,
  BrokerStatus,
  CreateTopicRequest,
} from './types'

class RustMQClient {
  private client: AxiosInstance

  constructor(baseURL: string = '') {
    this.client = axios.create({
      baseURL,
      timeout: 10000,
      headers: {
        'Content-Type': 'application/json',
      },
    })
  }

  // Health Check
  async getHealth(): Promise<HealthResponse> {
    const response = await this.client.get<HealthResponse>('/health')
    return response.data
  }

  // Cluster Management
  async getClusterStatus(): Promise<ClusterStatus> {
    const response = await this.client.get<ApiResponse<ClusterStatus>>('/api/v1/cluster')
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error || 'Failed to get cluster status')
    }
    return response.data.data
  }

  // Topics Management
  async listTopics(): Promise<TopicSummary[]> {
    const response = await this.client.get<ApiResponse<TopicSummary[]>>('/api/v1/topics')
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error || 'Failed to list topics')
    }
    return response.data.data
  }

  async getTopic(name: string): Promise<TopicDetail> {
    const response = await this.client.get<ApiResponse<TopicDetail>>(`/api/v1/topics/${name}`)
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error || 'Failed to get topic')
    }
    return response.data.data
  }

  async createTopic(request: CreateTopicRequest): Promise<string> {
    const response = await this.client.post<ApiResponse<string>>('/api/v1/topics', request)
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error || 'Failed to create topic')
    }
    return response.data.data
  }

  async deleteTopic(name: string): Promise<string> {
    const response = await this.client.delete<ApiResponse<string>>(`/api/v1/topics/${name}`)
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error || 'Failed to delete topic')
    }
    return response.data.data
  }

  // Brokers Management
  async listBrokers(): Promise<BrokerStatus[]> {
    const response = await this.client.get<ApiResponse<BrokerStatus[]>>('/api/v1/brokers')
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error || 'Failed to list brokers')
    }
    return response.data.data
  }
}

// Export singleton instance
export const api = new RustMQClient()
export default api
