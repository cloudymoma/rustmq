# RustMQ Web UI

A modern, responsive web interface for managing and monitoring RustMQ clusters built with Vue.js 3 and TypeScript.

## Features

### ✅ Implemented (v1.0.0)

- **📊 Dashboard**: Real-time cluster overview with health metrics
  - Cluster health status and Raft term information
  - Broker statistics (online/offline counts)
  - Topic statistics (total topics, partitions, replication factor)
  - Recent topics list
  - Broker health monitoring with auto-refresh

- **📝 Topics Management**: Full CRUD operations for topics
  - List all topics with partition and replication details
  - Create new topics with configurable settings
  - Delete existing topics
  - View detailed topic information including partition assignments
  - Configure retention, compression, and segment settings

- **🖥️ Brokers Monitoring**: Real-time broker health tracking
  - Broker status overview (online/offline)
  - Health percentage calculation
  - Individual broker health cards
  - Auto-refresh broker status every 5 seconds

- **🔒 ACL Viewer**: Placeholder for future ACL management
  - Security status overview
  - Best practices documentation
  - Coming soon: Full ACL management interface

- **⚙️ Configuration**: View cluster and system configuration
  - Cluster configuration (leader, term, broker count)
  - Storage configuration (WAL, object storage, cache)
  - Network configuration (QUIC, gRPC, Admin API ports)
  - Performance features showcase
  - System information (version, uptime, health)

## Tech Stack

- **Framework**: Vue.js 3 (Composition API with `<script setup>`)
- **Language**: TypeScript
- **Build Tool**: Vite
- **HTTP Client**: Axios
- **Routing**: Vue Router 4
- **Styling**: Custom CSS with dark theme

## Getting Started

### Prerequisites

- Node.js 18+ and npm (or yarn/pnpm)
- RustMQ admin server running on port 8080 (configurable)

### Installation

1. Install dependencies:

```bash
cd web
npm install
```

### Development

Run the development server with hot-reload:

```bash
npm run dev
```

The WebUI will be available at `http://localhost:3000` with API proxy to `localhost:8080`.

### Build for Production

Build the application for production deployment:

```bash
npm run build
```

This creates an optimized build in the `dist/` directory.

### Preview Production Build

Test the production build locally:

```bash
npm run preview
```

## Project Structure

```
web/
├── src/
│   ├── api/                    # API client and types
│   │   ├── client.ts          # Axios-based API client
│   │   └── types.ts           # TypeScript interfaces for API
│   ├── views/                 # Vue components for each page
│   │   ├── Dashboard.vue      # Cluster overview
│   │   ├── Topics.vue         # Topic management
│   │   ├── Brokers.vue        # Broker monitoring
│   │   ├── ACL.vue            # ACL viewer (placeholder)
│   │   └── Config.vue         # Configuration viewer
│   ├── App.vue                # Root component with navigation
│   ├── router.ts              # Vue Router configuration
│   ├── main.ts                # Application entry point
│   └── style.css              # Global styles
├── public/                    # Static assets
├── dist/                      # Production build output (generated)
├── index.html                 # HTML template
├── package.json               # Dependencies and scripts
├── vite.config.ts             # Vite configuration
├── tsconfig.json              # TypeScript configuration
└── README.md                  # This file
```

## API Integration

The WebUI communicates with the RustMQ admin server REST API:

### Endpoints Used

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check and uptime |
| `/api/v1/cluster` | GET | Cluster status and metadata |
| `/api/v1/topics` | GET | List all topics |
| `/api/v1/topics` | POST | Create a new topic |
| `/api/v1/topics/{name}` | GET | Get topic details |
| `/api/v1/topics/{name}` | DELETE | Delete a topic |
| `/api/v1/brokers` | GET | List all brokers |

### Type Safety

All API responses are fully typed using TypeScript interfaces defined in `src/api/types.ts`. This provides:
- Compile-time type checking
- IntelliSense/autocomplete in editors
- Runtime validation with error handling

Example:

```typescript
interface ClusterStatus {
  brokers: BrokerStatus[]
  topics: TopicSummary[]
  leader: string | null
  term: number
  healthy: boolean
}
```

## Configuration

### Development Proxy

The dev server proxies API requests to avoid CORS issues. Configure in `vite.config.ts`:

```typescript
server: {
  proxy: {
    '/api': {
      target: 'http://localhost:8080',  // Change if admin server runs elsewhere
      changeOrigin: true,
    },
  },
}
```

### Production Deployment

For production, the WebUI is served directly by the RustMQ admin server (no separate web server needed):

1. Build the WebUI:
   ```bash
   cd web
   npm run build
   ```

2. Start the admin server:
   ```bash
   cargo run --bin admin_server
   ```

3. Access the WebUI at:
   ```
   http://localhost:8080
   ```

The admin server automatically serves static files from `web/dist/` directory.

## Features in Detail

### Dashboard

- **Auto-refresh**: Cluster status updates every 5 seconds
- **Metrics Cards**:
  - Cluster health (healthy/unhealthy indicator)
  - Leader node and Raft term
  - Broker statistics (total/online/offline)
  - Topic statistics (total topics, partitions, avg replication)
- **Recent Topics**: Shows the 5 most recently created topics
- **Broker List**: Full table of all brokers with status

### Topics Management

- **Create Topic Form**:
  - Topic name validation (alphanumeric, hyphens, underscores)
  - Configurable partitions and replication factor
  - Optional retention period (in hours)
  - Compression type selection (none, gzip, lz4, snappy, zstd)

- **Topic Details Modal**:
  - Partition assignments (leader, replicas, in-sync replicas)
  - Configuration details
  - Created timestamp

- **Delete Confirmation**: Prevents accidental deletion

### Brokers Monitoring

- **Health Overview**:
  - Total broker count
  - Online/offline breakdown
  - Overall health percentage

- **Broker Cards**:
  - Visual health status (green/red border)
  - Endpoint information
  - Current status

- **Auto-refresh**: Updates every 5 seconds

### Configuration Viewer

- **Cluster Info**: Leader, term, broker count
- **Storage Details**: WAL backend, object storage type, cache strategy
- **Network Ports**: QUIC, gRPC, Admin API, Controller
- **Performance Features**: Showcases RustMQ optimizations
  - SmallVec (90% allocation reduction)
  - Buffer pooling (30-40% overhead reduction)
  - Moka cache (lock-free TinyLFU)
  - FuturesUnordered (85% latency improvement)
  - Fast ACL (547ns checks)
  - Miri validation (memory safety)

## Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## Development Tips

### Type Checking

Run TypeScript type checker without building:

```bash
npm run type-check
```

### Hot Module Replacement (HMR)

Vite provides instant HMR during development. Changes to Vue components and TypeScript files are reflected immediately without full page reloads.

### Debugging

1. Open browser DevTools
2. Vue DevTools extension recommended for component debugging
3. Network tab shows all API requests/responses

### Adding New Features

1. **Define TypeScript types** in `src/api/types.ts`
2. **Add API methods** in `src/api/client.ts`
3. **Create/update Vue component** in `src/views/`
4. **Add route** in `src/router.ts` if needed
5. **Update navigation** in `src/App.vue` if needed

## Styling Guidelines

- Uses custom CSS with CSS variables for theming
- Dark theme by default (light theme can be added)
- Responsive design with mobile breakpoints
- Consistent spacing and typography
- Status indicators: green (online), red (offline), yellow (warning)

### Color Palette

- Primary: `#646cff` (blue)
- Success: `#4ade80` (green)
- Error: `#ef4444` (red)
- Warning: `#fbbf24` (yellow)
- Background: `#242424`
- Card background: `#1a1a1a`
- Border: `#3f3f3f`

## Security

⚠️ **Important**: The current WebUI has no authentication/authorization.

For production deployments:
1. Enable mTLS on the admin server
2. Configure firewall rules to restrict access
3. Use reverse proxy with authentication (nginx, Caddy, etc.)
4. Monitor access logs

Future versions will include:
- Built-in authentication
- Role-based access control (RBAC)
- Session management
- Audit logging

## Troubleshooting

### Build Errors

**Issue**: `Cannot find module 'vue'`
```bash
# Solution: Reinstall dependencies
rm -rf node_modules package-lock.json
npm install
```

**Issue**: TypeScript errors during build
```bash
# Solution: Run type checker first
npm run type-check
```

### Runtime Errors

**Issue**: API requests fail (CORS or connection refused)
```bash
# Solution: Ensure admin server is running
cargo run --bin admin_server

# Check server is accessible
curl http://localhost:8080/health
```

**Issue**: Page shows "Loading..." forever
- Check browser console for errors
- Verify API endpoints are responding
- Check network tab in DevTools

### Development Server Issues

**Issue**: Port 3000 already in use
```bash
# Solution: Change port in vite.config.ts
server: {
  port: 3001,  // Use different port
}
```

## Performance

- **Bundle Size**: ~150KB gzipped (production build)
- **Initial Load**: < 2s on fast 3G
- **Time to Interactive**: < 3s
- **Auto-refresh**: 5s interval for dashboard and brokers

## Future Enhancements

- [ ] Real-time WebSocket updates instead of polling
- [ ] Full ACL management interface
- [ ] Producer/Consumer monitoring and metrics
- [ ] Partition rebalancing controls
- [ ] Cluster configuration updates
- [ ] Grafana dashboard embedding
- [ ] Dark/Light theme toggle
- [ ] Advanced filtering and search
- [ ] Export data (CSV, JSON)
- [ ] Multi-cluster management
- [ ] Alert configuration
- [ ] Cost analytics dashboard

## Contributing

To contribute to the WebUI:

1. Follow Vue.js 3 Composition API patterns
2. Use TypeScript for all new code
3. Add types to `src/api/types.ts` for new API endpoints
4. Test in both development and production builds
5. Ensure responsive design works on mobile
6. Update this README for new features

## License

Copyright 2025 RustMQ Project

Licensed under the Apache License, Version 2.0. See the LICENSE file in the project root for details.

## Support

- **Documentation**: [Project README](../README.md)
- **Issues**: [GitHub Issues](https://github.com/rustmq/rustmq/issues)
- **Discussions**: [GitHub Discussions](https://github.com/rustmq/rustmq/discussions)

---

**Built with ❤️ using Vue.js, TypeScript, and Vite**
