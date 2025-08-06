# Security Architecture Guide

## Overview

This document outlines the comprehensive security architecture implemented for the Envoy Control Plane Rust application, focusing on JWT secret management, authentication, and authorization.

## Security Components

### 1. JWT Secret Management

#### **Environment-First Architecture**
- **Primary**: Environment variables (`JWT_SECRET`)
- **Secondary**: Docker secrets (`/run/secrets/jwt_secret`)
- **Fallback**: Configuration validation prevents insecure defaults

#### **Production Security Requirements**
```bash
# Required environment variables for production
JWT_SECRET="your-secure-256-bit-secret-minimum-32-chars"
JWT_ISSUER="your-service-name"
JWT_EXPIRY_HOURS=24
BCRYPT_COST=12
```

#### **Docker Secrets Integration**
```yaml
# docker-compose.yml
services:
  control-plane:
    secrets:
      - jwt_secret
      - admin_password
    environment:
      - AUTHENTICATION_ENABLED=true

secrets:
  jwt_secret:
    file: ./secrets/jwt_secret.txt
  admin_password:
    file: ./secrets/admin_password.txt
```

### 2. Configuration Security

#### **Secure Configuration Loading Priority**
1. **Docker Secrets** (Highest priority)
2. **Environment Variables** 
3. **Configuration File** (Lowest priority, no secrets)

#### **Security Validation**
- JWT secrets must be minimum 32 characters
- Bcrypt cost between 10-15
- JWT expiry between 1-168 hours
- Production deployment validation

### 3. Demo User Security

#### **Environment-Based Credentials**
```bash
# Secure demo user configuration
DEMO_ADMIN_USERNAME=admin
DEMO_ADMIN_PASSWORD=secure-generated-password-123
DEMO_USER1_USERNAME=user
DEMO_USER1_PASSWORD=secure-user-password-456
```

#### **Secure Defaults**
- Bcrypt-hashed passwords
- Random secure passwords when not configured
- Clear warnings for default credentials

### 4. JWT Token Rotation

#### **Rotation Manager**
- Periodic secret rotation capability
- Zero-downtime rotation
- Configurable rotation intervals
- Secure secret generation

#### **Rotation Configuration**
```bash
JWT_ROTATION_ENABLED=true
JWT_ROTATION_INTERVAL_HOURS=168  # Weekly
JWT_ROTATION_OVERLAP_MINUTES=60  # 1 hour overlap
JWT_SECRET_ROTATION=new-secret-for-rotation
```

## Security Best Practices

### 1. Secret Management

#### **DO**
✅ Use environment variables for all secrets
✅ Use Docker secrets in container environments
✅ Implement secret rotation in production
✅ Use strong, randomly generated secrets (64+ chars)
✅ Validate secret strength at startup

#### **DON'T**
❌ Store secrets in configuration files
❌ Use default/example secrets in production
❌ Log sensitive information
❌ Use short or predictable secrets

### 2. Authentication & Authorization

#### **JWT Token Security**
- HS256 algorithm (secure for symmetric keys)
- Short expiration times (24 hours default)
- Proper issuer validation
- Secure secret storage

#### **Password Security**
- Bcrypt hashing with cost 12+
- Environment-based credentials
- No plaintext password storage
- Secure password generation for defaults

### 3. Container Security

#### **Docker Configuration**
```dockerfile
# Use non-root user
USER appuser

# Secure file permissions
RUN chown -R appuser:appuser /app

# Health checks
HEALTHCHECK --interval=30s --timeout=3s \
  CMD curl -f http://localhost:8080/health || exit 1
```

#### **Kubernetes Security**
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: jwt-secret
type: Opaque
data:
  jwt_secret: <base64-encoded-secret>
---
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - name: control-plane
        env:
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: jwt-secret
              key: jwt_secret
```

## Development vs Production

### Development Environment
```bash
# Relaxed settings for development
AUTHENTICATION_ENABLED=false
JWT_SECRET=development-secret-minimum-32-chars
BCRYPT_COST=8
```

### Production Environment
```bash
# Strict production settings
AUTHENTICATION_ENABLED=true
JWT_SECRET=${SECURE_JWT_SECRET}  # From secret management
BCRYPT_COST=12
JWT_ROTATION_ENABLED=true
```

## Monitoring & Security

### 1. Security Logging
- Authentication attempts
- Authorization failures
- Secret rotation events
- Configuration security warnings

### 2. Health Checks
- Secure health endpoint (no credential exposure)
- Authentication system status
- JWT configuration validation

### 3. Metrics
- Authentication success/failure rates
- Token expiration events
- Rotation completion status

## Migration Guide

### From Insecure to Secure Configuration

1. **Phase 1: Environment Variables**
   ```bash
   export JWT_SECRET="your-secure-production-secret"
   ```

2. **Phase 2: Docker Secrets**
   ```bash
   echo "your-secure-secret" | docker secret create jwt_secret -
   ```

3. **Phase 3: Enable Rotation**
   ```bash
   export JWT_ROTATION_ENABLED=true
   ```

## Testing Security

### Automated Security Tests
- JWT token validation
- Password hashing verification
- Environment variable security
- Configuration validation

### Security Checklist
- [ ] No hardcoded secrets in code
- [ ] Environment variables configured
- [ ] Docker secrets implemented
- [ ] Rotation capability tested
- [ ] Health endpoint secure
- [ ] Authentication flow tested
- [ ] Authorization permissions verified

## Troubleshooting

### Common Issues

#### "JWT_SECRET environment variable is required"
```bash
# Set the JWT secret
export JWT_SECRET="your-secure-secret-minimum-32-characters"
```

#### "JWT secret must be at least 32 characters long"
```bash
# Use a longer secret
export JWT_SECRET="your-very-secure-secret-that-is-at-least-32-characters-long"
```

#### Authentication disabled warnings
```bash
# Enable authentication
export AUTHENTICATION_ENABLED=true
```

## Security Contacts

For security issues or questions:
- Review this documentation
- Check application logs for security warnings
- Validate environment configuration
- Test authentication flows

---

**Last Updated**: 2025-08-04
**Version**: 1.0