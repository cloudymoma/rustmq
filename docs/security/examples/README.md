# RustMQ Security Examples

This directory contains practical examples and templates for implementing RustMQ security features. The examples are organized by category and deployment scenario.

## Directory Structure

```
examples/
├── README.md                    # This file
├── certificates/                # Certificate templates and examples
│   ├── ca-configs/             # Certificate Authority configurations
│   ├── cert-templates/         # Certificate templates for different roles
│   └── scripts/                # Certificate management scripts
├── acl-rules/                  # Access Control List examples
│   ├── basic-patterns/         # Common ACL patterns
│   ├── advanced-scenarios/     # Complex authorization scenarios
│   └── compliance/             # Compliance-focused ACL configurations
├── configurations/             # Complete security configurations
│   ├── development/            # Development environment configs
│   ├── staging/                # Staging environment configs
│   ├── production/             # Production environment configs
│   └── compliance/             # Compliance-specific configurations
├── kubernetes/                 # Kubernetes security manifests
│   ├── basic-deployment/       # Basic secure Kubernetes deployment
│   ├── production-deployment/  # Production-ready deployment
│   └── service-mesh/           # Service mesh integration examples
└── scripts/                    # Automation and maintenance scripts
    ├── deployment/             # Deployment automation
    ├── monitoring/             # Security monitoring scripts
    └── maintenance/            # Ongoing maintenance scripts
```

## Quick Start Examples

### Basic Development Setup
```bash
# Copy development configuration
cp configurations/development/security-config.toml /etc/rustmq/config/

# Generate development certificates
./certificates/scripts/generate-dev-certs.sh

# Apply basic ACL rules
rustmq-admin acl import --input-file acl-rules/basic-patterns/development-acls.json
```

### Production Deployment
```bash
# Use production configuration template
cp configurations/production/security-config.toml /etc/rustmq/config/

# Deploy with Kubernetes
kubectl apply -f kubernetes/production-deployment/

# Configure production ACL rules
rustmq-admin acl import --input-file acl-rules/advanced-scenarios/production-acls.json
```

### Compliance Setup (SOX/PCI-DSS)
```bash
# Apply compliance configuration
cp configurations/compliance/pci-dss-config.toml /etc/rustmq/config/

# Use compliance ACL rules
rustmq-admin acl import --input-file acl-rules/compliance/pci-dss-acls.json

# Set up compliance monitoring
./scripts/monitoring/setup-compliance-monitoring.sh
```

## Usage Guidelines

1. **Never use examples directly in production** - Always review and customize for your environment
2. **Update passwords and keys** - Replace all placeholder values with secure, unique values
3. **Review permissions** - Ensure ACL rules follow the principle of least privilege
4. **Test thoroughly** - Validate all configurations in a staging environment first
5. **Keep updated** - Regularly update configurations to match security best practices

## Customization

Most examples include placeholder values that must be replaced:

- `{{COMPANY_NAME}}` - Your organization name
- `{{ENVIRONMENT}}` - Environment name (dev, staging, prod)
- `{{DOMAIN}}` - Your domain name
- `{{IP_RANGE}}` - Your network IP ranges
- `{{CA_PASSWORD}}` - Certificate Authority password
- `{{SERVICE_ACCOUNT}}` - Service account names

Use the provided scripts or manual replacement to customize examples for your environment.

## Security Notes

- All private keys in examples are for demonstration only
- Certificate examples use placeholder data
- Production deployments should use Hardware Security Modules (HSMs) for CA keys
- Regularly rotate certificates and review ACL rules
- Follow your organization's security policies and compliance requirements

## Support

For questions about these examples:
1. Check the main security documentation
2. Review the troubleshooting guide
3. Consult your security team
4. Contact RustMQ support for enterprise deployments