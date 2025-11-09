# Multi-Tenant Office 365 Audit Log Collection

## Overview

The Office 365 Audit Log Collector now supports simultaneous collection from **multiple Office 365 tenants** in a single process. This enables organizations managing multiple tenants to efficiently collect audit logs from all tenants concurrently.

## Key Features

- ✅ **Concurrent Collection**: Collect from multiple tenants simultaneously
- ✅ **Tenant Isolation**: Each tenant has independent authentication, rate limiting, and deduplication
- ✅ **Automatic Tagging**: All logs include `TenantName` and `TenantId` fields for easy filtering
- ✅ **Unified Output**: Send logs from all tenants to the same destination(s)
- ✅ **Per-Tenant Tracking**: Separate deduplication state prevents duplicate log collection
- ✅ **Backward Compatible**: Single-tenant CLI mode still works

## Configuration

### Multi-Tenant Mode

Define your tenants in the YAML configuration file:

```yaml
# config.yaml
tenants:
  - name: production
    tenantId: "12345678-1234-1234-1234-123456789012"
    clientId: "87654321-4321-4321-4321-210987654321"
    secretKey: "your-secret-key-here"
    publisherId: "12345678-1234-1234-1234-123456789012"  # Optional

  - name: development
    tenantId: "abcdef12-1234-1234-1234-123456789012"
    clientId: "fedcba21-4321-4321-4321-210987654321"
    secretKey: "another-secret-key"
    # publisherId optional - defaults to tenantId

collect:
  contentTypes:
    Audit.Exchange: True
    Audit.SharePoint: True
  hoursToCollect: 24

output:
  graylog:
    address: localhost
    port: 5555
```

### Running Multi-Tenant Collection

```bash
./office_audit_log_collector --config config.yaml
```

The collector will automatically detect the `tenants` section and run in multi-tenant mode.

## Single-Tenant Mode (Legacy)

For backward compatibility, you can still run single-tenant mode using CLI arguments:

```bash
./office_audit_log_collector \
  --tenant-id "12345678-1234-1234-1234-123456789012" \
  --client-id "87654321-4321-4321-4321-210987654321" \
  --secret-key "your-secret-key" \
  --config config.yaml
```

**Note**: In single-tenant mode, do NOT include a `tenants` section in the config file.

## Output Format

### Log Tagging

Every log collected in multi-tenant mode includes these additional fields:

```json
{
  "TenantName": "production",
  "TenantId": "12345678-1234-1234-1234-123456789012",
  "OriginFeed": "Audit.Exchange",
  "CreationTime": "2024-01-15T10:30:00Z",
  // ... rest of Office 365 audit log fields
}
```

### Graylog

Logs are sent to Graylog with tenant fields automatically included:

```json
{
  "TenantName": "production",
  "TenantId": "12345678-...",
  "message": "User signed in",
  // ... audit log data
}
```

**Filtering in Graylog:**
```
TenantName:production AND OriginFeed:Audit.Exchange
```

### Fluentd

Logs are tagged with tenant information:

```
office365.production.Audit.Exchange {"TenantName": "production", ...}
```

### CSV Output

CSV files include `TenantName` and `TenantId` columns:

```csv
TenantName,TenantId,OriginFeed,CreationTime,Operation,UserId,...
production,12345678-...,Audit.Exchange,2024-01-15T10:30:00Z,MailItemsAccessed,user@example.com,...
development,abcdef12-...,Audit.SharePoint,2024-01-15T10:31:00Z,FileUploaded,dev@example.com,...
```

## Deduplication

Each tenant maintains its own deduplication state in separate files:

```
known_blobs_production
known_blobs_development
known_blobs_staging
```

This ensures:
- No cross-tenant blob ID conflicts
- Independent tracking per tenant
- Proper deduplication even if tenants are added/removed

## Resource Considerations

### Memory Usage

Each tenant caches logs before outputting. Default is 500,000 logs per tenant:

- **3 tenants**: ~3GB RAM (3 × 500k × 2KB per log)
- **5 tenants**: ~5GB RAM
- **10 tenants**: ~10GB RAM

**Recommendation**: For 5+ tenants, reduce `cacheSize` to 100,000-200,000

```yaml
collect:
  cacheSize: 100000  # Reduced for many tenants
```

### Network Throughput

Each tenant runs up to `maxThreads` concurrent API requests (default: 50):

- **3 tenants**: 150 concurrent HTTP requests
- **5 tenants**: 250 concurrent requests

**Recommendation**: For 5+ tenants, reduce `maxThreads` to 25-30

```yaml
collect:
  maxThreads: 25  # Reduced for many tenants
```

### CPU Usage

The Tokio async runtime efficiently handles all tenants with minimal CPU overhead. CPU usage scales linearly with the number of API requests being processed.

## Rate Limiting

Office 365 API rate limits are **per-tenant**:

- Each tenant has independent rate limit tracking
- If one tenant hits a 429 (rate limit), only that tenant backs off for 30 seconds
- Other tenants continue collecting normally

## Performance Tuning

### Recommended Settings by Tenant Count

#### 1-3 Tenants (Default Settings)
```yaml
collect:
  cacheSize: 500000
  maxThreads: 50
  hoursToCollect: 24
```

#### 4-6 Tenants (Medium Scale)
```yaml
collect:
  cacheSize: 200000
  maxThreads: 30
  hoursToCollect: 24
```

#### 7-10 Tenants (Large Scale)
```yaml
collect:
  cacheSize: 100000
  maxThreads: 20
  hoursToCollect: 24
```

#### 10+ Tenants (Very Large Scale)
Consider running multiple processes or sequential collection:
```yaml
collect:
  cacheSize: 50000
  maxThreads: 15
  hoursToCollect: 12  # Shorter windows
```

## Migration from Single to Multi-Tenant

### Step 1: Update Configuration

Add a `tenants` section to your config:

```yaml
tenants:
  - name: my-tenant  # Choose a descriptive name
    tenantId: "xxx"  # From your old --tenant-id
    clientId: "yyy"  # From your old --client-id
    secretKey: "zzz" # From your old --secret-key

# Keep your existing collect/output sections
collect:
  # ...
output:
  # ...
```

### Step 2: Rename Known Blobs File

Rename your existing deduplication file:

```bash
mv known_blobs known_blobs_my-tenant
```

Where `my-tenant` matches the `name` field in your config.

### Step 3: Update Your Run Command

```bash
# Old way
./collector --tenant-id xxx --client-id yyy --secret-key zzz --config config.yaml

# New way
./collector --config config.yaml
```

### Step 4: Add More Tenants

Simply add more entries to the `tenants` list:

```yaml
tenants:
  - name: my-tenant
    # ... existing tenant

  - name: another-tenant
    tenantId: "new-tenant-id"
    clientId: "new-client-id"
    secretKey: "new-secret"
```

## Troubleshooting

### Issue: "No tenants configured in config file"

**Cause**: Config file doesn't have a `tenants` section and no CLI args provided.

**Solution**: Either add `tenants` section to config, or use CLI args for single-tenant mode.

### Issue: High memory usage with many tenants

**Cause**: Default `cacheSize` (500k) is too large for many tenants.

**Solution**: Reduce `cacheSize` in config:
```yaml
collect:
  cacheSize: 100000  # Lower value
```

### Issue: Logs from different tenants mixed together

**Cause**: This is expected behavior - all tenants send to the same output.

**Solution**: Use the `TenantName` or `TenantId` fields to filter:
- Graylog: `TenantName:production`
- Splunk: `TenantName="production"`
- CSV: Filter the `TenantName` column

### Issue: Rate limiting affecting all tenants

**Cause**: You're seeing rate limits but other tenants stop too.

**Solution**: This shouldn't happen - rate limits are per-tenant. Check your network/firewall settings.

### Issue: Want different outputs per tenant

**Current Limitation**: All tenants share the same output configuration.

**Workaround**:
1. Run separate processes with different configs per tenant group, OR
2. Filter at the destination (e.g., Graylog streams) based on `TenantName`

## Security Considerations

### Credential Storage

Tenant secrets are stored in the YAML config file:

```bash
# Protect your config file
chmod 600 config.yaml
```

**Recommendation**: Use environment variables (future enhancement) or a secrets management system.

### Tenant Isolation

- Each tenant uses separate API connections
- No shared authentication tokens between tenants
- Separate deduplication state prevents cross-tenant data leaks

### Network Security

All API communication is over HTTPS to Microsoft endpoints:
- `https://login.microsoftonline.com` (authentication)
- `https://manage.office.com` (audit logs)

## Examples

See the `Release/ConfigExamples/` directory for complete examples:

- `multiTenantGraylog.yaml` - Basic multi-tenant setup with Graylog
- `multiTenantFull.yaml` - Complete configuration with all options

## Comparison: Single vs Multi-Tenant Mode

| Feature | Single-Tenant | Multi-Tenant |
|---------|---------------|--------------|
| Configuration | CLI args | YAML config |
| Tenants | 1 | Unlimited |
| Execution | One process per tenant | One process for all |
| Deduplication | `known_blobs` | `known_blobs_{name}` |
| Log tagging | None | `TenantName`, `TenantId` |
| Resource usage | Low | Scales with tenant count |
| Rate limiting | Single tenant | Per-tenant isolation |

## Future Enhancements

Planned improvements for multi-tenant support:

1. **Per-Tenant Settings**: Override collection settings per tenant
2. **Tenant-Specific Outputs**: Route different tenants to different destinations
3. **Environment Variable Secrets**: Use env vars instead of config file secrets
4. **Managed Identity Support**: Azure Managed Identity authentication
5. **Dynamic Tenant Management**: Add/remove tenants without restart
6. **Per-Tenant Metrics**: Prometheus metrics broken down by tenant

## Need Help?

- **Documentation**: See main README.md for setup instructions
- **Issues**: https://github.com/ddbnl/office365-audit-log-collector/issues
- **Examples**: Check `Release/ConfigExamples/` directory
