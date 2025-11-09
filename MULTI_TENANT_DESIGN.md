# Multi-Tenant Architecture Design

## Overview
This document describes the multi-tenant rebuild of the Office 365 Audit Log Collector, enabling simultaneous collection from multiple Office 365 tenants with a single process.

## Design Goals
1. **Concurrent Collection**: Collect logs from multiple tenants simultaneously
2. **Tenant Isolation**: Each tenant has isolated authentication, rate limiting, and deduplication
3. **Unified Output**: All logs flow to the same output destinations but are tagged by tenant
4. **Backward Compatibility**: Support legacy single-tenant CLI mode
5. **Scalability**: Handle 10+ tenants efficiently

## Configuration Structure

### New Multi-Tenant Config Format
```yaml
# Define multiple tenants
tenants:
  - name: contoso
    tenantId: "12345678-1234-1234-1234-123456789012"
    clientId: "87654321-4321-4321-4321-210987654321"
    secretKey: "your-secret-key-here"
    publisherId: "12345678-1234-1234-1234-123456789012"  # optional, defaults to tenantId

  - name: fabrikam
    tenantId: "abcdef12-1234-1234-1234-123456789012"
    clientId: "fedcba21-4321-4321-4321-210987654321"
    secretKey: "another-secret-key"

# Shared collection settings (apply to all tenants)
collect:
  workingDir: ./
  contentTypes:
    Audit.General: True
    Audit.AzureActiveDirectory: True
    Audit.Exchange: True
    Audit.SharePoint: True
    DLP.All: True
  cacheSize: 500000
  maxThreads: 50
  globalTimeout: 60
  retries: 3
  skipKnownLogs: True
  hoursToCollect: 24

# Shared output settings
output:
  graylog:
    address: localhost
    port: 5555
```

### Backward Compatible: Single Tenant CLI Mode
```bash
# Old way still works
./collector --tenant-id "xxx" --client-id "yyy" --secret-key "zzz" --config config.yaml

# Config file without tenants section uses CLI args
```

## Architecture Changes

### 1. Configuration Module (config.rs)

#### New Structures
```rust
pub struct TenantConfig {
    pub name: String,
    pub tenant_id: String,
    pub client_id: String,
    pub secret_key: String,
    pub publisher_id: Option<String>,
}

pub struct Config {
    pub tenants: Option<Vec<TenantConfig>>,  // NEW: Multi-tenant support
    pub log: Option<LogSubConfig>,
    pub collect: CollectSubConfig,
    pub output: OutputSubConfig,
}
```

#### Known Blobs Per Tenant
- File naming: `known_blobs_{tenant_name}`
- Each tenant maintains separate deduplication state
- Prevents cross-tenant blob ID conflicts

### 2. CLI Args (data_structures.rs)

#### Modified CliArgs
```rust
pub struct CliArgs {
    // Single tenant mode (optional, for backward compatibility)
    #[arg(long)]
    pub tenant_id: Option<String>,

    #[arg(long)]
    pub client_id: Option<String>,

    #[arg(long)]
    pub secret_key: Option<String>,

    // Multi-tenant mode flag
    #[arg(long)]
    pub multi_tenant: bool,

    // Common args
    #[arg(long)]
    pub config: String,

    // ... rest
}
```

### 3. Multi-Tenant Collector (new: multi_tenant_collector.rs)

#### Orchestration Strategy
```rust
pub struct MultiTenantCollector {
    config: Config,
    tenant_collectors: Vec<TenantCollectorHandle>,
}

struct TenantCollectorHandle {
    tenant_name: String,
    collector: Collector,
    handle: JoinHandle<()>,
}
```

**Execution Model**: Parallel
- Each tenant runs in its own async task
- Shared Tokio runtime across all tenants
- Independent rate limiting per tenant
- Aggregated statistics at the end

### 4. Log Tagging

#### Tenant Identification in Logs
Every log gets additional fields:
```json
{
  "TenantName": "contoso",
  "TenantId": "12345678-1234-1234-1234-123456789012",
  "OriginFeed": "Audit.Exchange",
  // ... original log fields
}
```

### 5. Output Interface Updates

#### Graylog
- Add `TenantName` and `TenantId` fields to each log
- Maintain single TCP connection per output

#### Fluentd
- Existing `tenantName` becomes dynamic per log source
- Tag format: `office365.{tenant_name}.{content_type}`

#### CSV
- New columns: `TenantName`, `TenantId`
- Option: `separateByTenant` to create per-tenant CSV files

#### Azure Log Analytics
- Table naming: `{TenantName}_{ContentType}` (e.g., `Contoso_Audit_Exchange`)
- Or single table with TenantId column

## Implementation Plan

### Phase 1: Configuration Foundation
1. Add `TenantConfig` struct
2. Update `Config` deserialization
3. Implement per-tenant known_blobs files
4. Update CLI args parsing

### Phase 2: Collector Orchestration
1. Create `MultiTenantCollector`
2. Modify existing `Collector` to accept tenant context
3. Implement parallel execution model
4. Add tenant identification to all logs

### Phase 3: Output Interfaces
1. Update all interfaces to handle tenant fields
2. Test Graylog with multi-tenant logs
3. Implement per-tenant CSV option
4. Update Azure OMS table naming

### Phase 4: Testing & Documentation
1. Create example multi-tenant config
2. Test with 2+ tenants
3. Update README
4. Performance testing

## Performance Considerations

### Resource Usage
- **Memory**: Each tenant caches up to `cacheSize` logs (default 500k)
  - 5 tenants × 500k logs × ~2KB/log ≈ 5GB RAM
- **Network**: Each tenant runs up to `maxThreads` concurrent requests (default 50)
  - 5 tenants × 50 threads = 250 concurrent HTTP requests
- **CPU**: Tokio runtime handles all async tasks efficiently

### Scaling Recommendations
- **1-5 tenants**: Default settings work well
- **6-10 tenants**: Reduce `cacheSize` to 100k, `maxThreads` to 25
- **10+ tenants**: Consider running multiple processes or sequential collection

### Rate Limiting
- Office 365 API rate limits are per-tenant
- Each tenant has independent rate limit tracking
- Global backoff: If any tenant hits 429, that tenant waits 30s (others continue)

## Migration Path

### From Single to Multi-Tenant

**Step 1**: Update config file
```yaml
# Add tenants section
tenants:
  - name: existing-tenant
    tenantId: "..."  # from old CLI args
    clientId: "..."
    secretKey: "..."

# Keep existing collect/output sections
```

**Step 2**: Update execution
```bash
# Old way (deprecated but still works)
./collector --tenant-id "xxx" --client-id "yyy" --secret-key "zzz" --config config.yaml

# New way
./collector --config config.yaml
```

**Step 3**: Migrate known_blobs
```bash
# Rename existing file
mv known_blobs known_blobs_existing-tenant
```

## Security Considerations

1. **Credential Storage**: Secrets in config file should be protected
   - Use file permissions (chmod 600)
   - Consider environment variable substitution
   - Future: Support Azure Key Vault integration

2. **Tenant Isolation**:
   - Each tenant uses separate API connections
   - No shared authentication tokens
   - Separate deduplication state prevents cross-tenant data leaks

3. **Output Security**:
   - Ensure downstream systems (Graylog) can handle multi-tenant data
   - Consider output-level tenant filtering if needed

## Future Enhancements

1. **Per-Tenant Collection Settings**: Override global settings per tenant
2. **Dynamic Tenant Management**: Add/remove tenants without restart
3. **Tenant-Specific Outputs**: Route different tenants to different destinations
4. **Managed Identity Support**: Use Azure Managed Identity instead of client secrets
5. **Metrics & Monitoring**: Per-tenant statistics, Prometheus metrics
6. **Scheduler Integration**: Built-in scheduling instead of requiring cron

## Breaking Changes

1. **Config Schema**: New `tenants` section (backward compatible if using CLI args)
2. **Known Blobs File**: Naming convention changes to include tenant name
3. **Log Format**: All logs now include `TenantName` and `TenantId` fields

## Rollback Plan

If issues occur:
1. Use CLI args for single-tenant mode (unchanged)
2. Config files without `tenants` section continue working
3. Old `known_blobs` file format still recognized (for single tenant)
