# Office 365 Audit Log Collector

A high-performance Rust application for collecting audit logs from one or more Office 365 tenants and forwarding them to logging platforms (Graylog, Fluentd, Azure Log Analytics, or CSV files).

## Features

- 🚀 **Multi-Tenant Support**: Collect from unlimited Office 365 tenants simultaneously
- ⚡ **High Performance**: Async Rust with concurrent processing (50 threads per tenant by default)
- ⏰ **Built-in Daemon Mode**: Internal scheduler with simple intervals (30m, 3h, 1d) or cron expressions
- 📧 **Email Notifications**: SMTP alerts on collection success/failure with detailed summaries
- 🔄 **Automatic Deduplication**: Per-tenant tracking prevents duplicate log collection
- 🏷️ **Automatic Tagging**: All logs tagged with `TenantName` and `TenantId`
- 📊 **Multiple Outputs**: Graylog, Fluentd, Azure Log Analytics, CSV
- 🔁 **Retry Logic**: Automatic retry with backoff for failed requests
- 🎯 **Content Filtering**: Optional filtering by log fields
- 💾 **Memory Efficient**: Configurable caching and batching
- 🐳 **Docker Ready**: Multi-stage build, runs as non-root user

## Quick Start

### 1. Prerequisites

- Rust toolchain (for building from source)
- OR Docker (to run pre-built container)
- Office 365 tenant(s) with admin access
- Azure App Registration(s) with appropriate permissions

### 2. Azure App Registration Setup

For each Office 365 tenant you want to collect from:

#### 2.1 Create App Registration

1. Go to [Azure Portal](https://portal.azure.com) → Azure Active Directory → App registrations
2. Click **"New registration"**
3. Name: `Office365-Audit-Collector`
4. Supported account types: **"Accounts in this organizational directory only"**
5. Click **Register**

#### 2.2 Grant API Permissions

1. Go to **API permissions** → **Add a permission**
2. Select **Office 365 Management APIs**
3. Select **Application permissions**
4. Add these permissions:
   - `ActivityFeed.Read`
   - `ActivityFeed.ReadDlp`
   - `ServiceHealth.Read`
5. Click **Add permissions**
6. Click **"Grant admin consent for [Your Tenant]"** ✅

#### 2.3 Create Client Secret

1. Go to **Certificates & secrets** → **New client secret**
2. Description: `audit-collector`
3. Expires: Choose appropriate duration (e.g., 12 months)
4. Click **Add**
5. **IMPORTANT**: Copy the secret **Value** immediately (it won't be shown again)

#### 2.4 Note Your Credentials

You'll need these three values for each tenant:
- **Tenant ID**: From the app registration Overview page
- **Application (client) ID**: From the app registration Overview page
- **Client Secret**: The value you just copied

### 3. Install & Build

#### Option A: Build from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/office365-audit-log-collector.git
cd office365-audit-log-collector

# Build release version
cargo build --release

# Binary will be at: target/release/office_audit_log_collector
```

#### Option B: Run with Docker

```bash
# Build Docker image
docker build -t office365-audit-collector .

# Run container (mount your config)
docker run -v $(pwd)/config.yaml:/app/config.yaml office365-audit-collector
```

### 4. Configure

Create a `config.yaml` file. See `config.minimal.yaml` for the simplest setup.

#### Single Tenant Daemon Example

```yaml
tenants:
  - name: production
    tenantId: "your-tenant-id-here"
    clientId: "your-client-id-here"
    secretKey: "your-client-secret-here"

# Schedule: run every 3 hours
schedule:
  interval: 3h  # Options: 30m, 1h, 3h, 6h, 12h, 1d

# Optional: Email notifications
notifications:
  email:
    enabled: true
    smtp:
      host: smtp.gmail.com
      port: 587
      username: alerts@company.com
      password: your-app-password
    to: security@company.com
    on: [failure]  # Only send on errors

collect:
  workingDir: /app/data
  contentTypes:
    Audit.Exchange: true
    Audit.SharePoint: true
    Audit.AzureActiveDirectory: true
  hoursToCollect: 24  # Look back 24 hours

output:
  graylog:
    address: graylog
    port: 5555
```

#### Multi-Tenant Example

```yaml
tenants:
  - name: production
    tenantId: "prod-tenant-id"
    clientId: "prod-client-id"
    secretKey: "prod-secret"

  - name: development
    tenantId: "dev-tenant-id"
    clientId: "dev-client-id"
    secretKey: "dev-secret"

  - name: staging
    tenantId: "staging-tenant-id"
    clientId: "staging-client-id"
    secretKey: "staging-secret"

schedule:
  interval: 3h
  # cron: "0 */3 * * *"  # Alternative: cron format

notifications:
  email:
    enabled: true
    smtp:
      host: smtp.gmail.com
      port: 587
      username: alerts@company.com
      password: your-app-password
    to: security@company.com
    on: [failure]

collect:
  workingDir: /app/data
  contentTypes:
    Audit.General: true
    Audit.AzureActiveDirectory: true
    Audit.Exchange: true
    Audit.SharePoint: true
    DLP.All: true
  cacheSize: 200000      # Logs to batch (lower for many tenants)
  maxThreads: 30         # Concurrent requests (lower for many tenants)
  hoursToCollect: 24
  skipKnownLogs: true    # Enable deduplication

output:
  graylog:
    address: graylog.example.com
    port: 5555
```

**See `config.minimal.yaml`, `config.example.yaml`, and `config.advanced.yaml` for more examples.**

### 5. Run

#### Daemon Mode (Recommended)

Runs continuously on schedule, collecting immediately at startup:

```bash
# From source
./target/release/office_audit_log_collector --config config.yaml

# Docker
docker run -v $(pwd)/config.yaml:/app/config.yaml:ro \
           -v $(pwd)/data:/app/data \
           office365-audit-collector

# Docker Compose (recommended)
docker-compose up -d
```

Requires `schedule:` section in config. Collects immediately on start, then waits for scheduled runs.

#### Run-Once Mode

Run collection manually and exit:

```bash
# From source
./target/release/office_audit_log_collector --config config.yaml --run-now

# Docker
docker exec office365-collector \
  /app/office_audit_log_collector --config /app/config.yaml --run-now
```

Ignores schedule, runs once immediately. Exits with code 0 on success, 1 on failure.

#### Interactive Mode (Testing)

Test configuration with real-time TUI:

```bash
./target/release/office_audit_log_collector --config config.yaml --interactive
```

Uses first tenant from config. Great for testing credentials and connectivity.

### 6. Verify Logs

Check your configured output (Graylog, Fluentd, CSV files) for incoming logs.

All logs will include:
```json
{
  "TenantName": "production",
  "TenantId": "12345678-1234-1234-1234-123456789012",
  "OriginFeed": "Audit.Exchange",
  "Operation": "MailItemsAccessed",
  // ... Office 365 audit fields
}
```

## Configuration Reference

### Tenants Section (Required)

```yaml
tenants:
  - name: string              # Human-readable name (required)
    tenantId: string          # Azure AD tenant ID (required)
    clientId: string          # App registration client ID (required)
    secretKey: string         # Client secret (required)
    publisherId: string       # Optional, defaults to tenantId
```

### Schedule Section (Required for Daemon Mode)

```yaml
schedule:
  # Simple interval (easiest)
  interval: 3h                # Options: 30m, 1h, 3h, 6h, 12h, 1d

  # OR Cron expression (more control)
  cron: "0 */3 * * *"         # Every 3 hours
  # cron: "0 9 * * *"         # Daily at 9 AM UTC
  # cron: "0 9 * * 1-5"       # Weekdays at 9 AM
```

Choose either `interval` OR `cron`, not both.

### Notifications Section (Optional)

```yaml
notifications:
  email:
    enabled: true             # Enable/disable email notifications
    smtp:
      host: smtp.gmail.com    # SMTP server hostname
      port: 587               # SMTP port (587=TLS, 465=SSL)
      username: user@mail.com # SMTP username
      password: secret        # SMTP password/app password
    from: sender@mail.com     # From address (optional, defaults to username)
    to: alerts@company.com    # Recipient email
    on: [failure]             # When to send: [success], [failure], or [success, failure]
```

Email notifications include:
- Summary of logs collected per tenant
- Success/failure status per tenant
- Total logs collected
- Next scheduled run time

### Collection Settings

```yaml
collect:
  workingDir: ./data          # Directory for deduplication files (default: ./)

  contentTypes:               # Which log types to collect
    Audit.General: True
    Audit.AzureActiveDirectory: True
    Audit.Exchange: True
    Audit.SharePoint: True
    DLP.All: True

  cacheSize: 500000           # Logs to cache before output (default: 500000)
  maxThreads: 50              # Concurrent API requests per tenant (default: 50)
  globalTimeout: 60           # Minutes before force exit, 0=disabled (default: 0)
  retries: 3                  # Retry attempts for failures (default: 3)
  skipKnownLogs: True         # Enable deduplication (default: True)
  hoursToCollect: 24          # Lookback window, max 168 (default: 24)

  # Optional: Filter logs by field values
  filter:
    Audit.Exchange:
      Operation: "MailItemsAccessed"
    # Other content types...
```

### Output Configurations

#### Graylog (Raw TCP)

```yaml
output:
  graylog:
    address: localhost
    port: 5555
```

#### Fluentd

```yaml
output:
  fluentd:
    tenantName: office365     # Base tag name
    address: localhost
    port: 24224
```

#### CSV File

```yaml
output:
  file:
    path: output.csv
    separateByContentType: True  # Create separate files per content type
    separator: ";"               # CSV delimiter
```

#### Azure Log Analytics

```yaml
output:
  azureLogAnalytics:
    workspaceId: "your-workspace-id"

# Requires CLI argument: --oms-key "your-workspace-key"
```

### Logging (Optional)

```yaml
log:
  path: collector.log
  debug: False  # Set to True for debug logging
```

## Performance Tuning

### Memory Usage

Each tenant caches up to `cacheSize` logs (default 500k):
- **Memory per tenant** ≈ `cacheSize × 2KB`
- **3 tenants** with default settings: ~3GB RAM
- **10 tenants** with default settings: ~10GB RAM

**Recommendations:**
- **1-3 tenants**: Default settings (500k cache, 50 threads)
- **4-6 tenants**: Reduce to 200k cache, 30 threads
- **7-10 tenants**: Reduce to 100k cache, 20 threads
- **10+ tenants**: Consider 50k cache, 15 threads

### Network Throughput

- Each tenant uses up to `maxThreads` concurrent connections (default 50)
- **5 tenants × 50 threads = 250 concurrent HTTP requests**

For large deployments, reduce `maxThreads` to avoid overwhelming your network.

### Rate Limiting

- Office 365 rate limits are **per-tenant**
- If one tenant hits a rate limit, only that tenant backs off (30 seconds)
- Other tenants continue normally
- No special configuration needed

## Scheduling

### Built-in Daemon Scheduler (Recommended)

The collector has a built-in scheduler - no external cron needed:

```yaml
schedule:
  interval: 3h  # Simple: 30m, 1h, 3h, 6h, 12h, 1d
  # OR
  cron: "0 */3 * * *"  # Cron: Every 3 hours at minute 0
```

**Simple Intervals:**
- `30m` - Every 30 minutes
- `1h` - Every hour
- `3h` - Every 3 hours (recommended)
- `6h` - Every 6 hours
- `12h` - Twice daily
- `1d` - Once daily

**Cron Examples:**
- `"0 */3 * * *"` - Every 3 hours
- `"0 9 * * *"` - Daily at 9 AM UTC
- `"0 9 * * 1-5"` - Weekdays at 9 AM UTC
- `"*/30 * * * *"` - Every 30 minutes

**Behavior:**
- Collects immediately on startup (no initial delay)
- Then waits for next scheduled time
- Logs next scheduled run after each collection
- Email notifications include next run time

### Manual Trigger (--run-now)

For manual/on-demand collection:

```bash
# Run immediately, ignore schedule
./office_audit_log_collector --config config.yaml --run-now
```

Use cases:
- Manual data backfill
- Testing configuration
- Triggered by external event
- Kubernetes Job (not CronJob)

### External Scheduling (Alternative)

You can still use external schedulers if preferred:

```bash
# Disable daemon mode, use --run-now with cron
0 * * * * /path/to/office_audit_log_collector --config /path/to/config.yaml --run-now
```

**Note:** Config file doesn't need `schedule:` section when using `--run-now`.

## Deduplication

Each tenant maintains a separate deduplication file:

```
working_dir/
  ├── known_blobs_production
  ├── known_blobs_development
  └── known_blobs_staging
```

These files track which log blobs have been collected to prevent duplicates.

**Important:**
- Files are automatically created on first run
- Automatically cleaned (expired entries removed)
- **Do not delete** unless you want to re-collect all logs

## Email Notifications

Send automatic summary emails after each collection run.

### Setup

```yaml
notifications:
  email:
    enabled: true
    smtp:
      host: smtp.gmail.com
      port: 587
      username: your-email@gmail.com
      password: your-app-password  # For Gmail: App-specific password
    from: collector@company.com    # Optional, defaults to username
    to: security@company.com
    on: [failure]                  # When to send
```

### Notification Triggers

- `[failure]` - Send only when collection fails (recommended)
- `[success]` - Send only when collection succeeds
- `[success, failure]` - Send always

### Email Content

Subject: `Office 365 Audit Collection Summary - SUCCESS` or `FAILURE`

Body includes:
- Overall status (success/failure)
- Total logs collected
- Per-tenant breakdown:
  - ✓ tenant-name: 1,234 logs
  - ✗ tenant-name: Error message
- Collection duration
- Next scheduled run time

### SMTP Configuration Examples

**Gmail:**
```yaml
smtp:
  host: smtp.gmail.com
  port: 587
  username: your-email@gmail.com
  password: your-app-password  # Generate at: https://myaccount.google.com/apppasswords
```

**Office 365:**
```yaml
smtp:
  host: smtp.office365.com
  port: 587
  username: sender@company.com
  password: your-password
```

**SendGrid:**
```yaml
smtp:
  host: smtp.sendgrid.net
  port: 587
  username: apikey
  password: your-sendgrid-api-key
```

## Troubleshooting

### "Schedule configuration required for daemon mode"

**Cause**: Running without `--run-now` flag and no `schedule:` section in config.

**Fix**: Add schedule to config:
```yaml
schedule:
  interval: 3h
```

Or use `--run-now` to run once:
```bash
./office_audit_log_collector --config config.yaml --run-now
```

### "No tenants configured in config file"

**Cause**: Missing or empty `tenants:` section in config.

**Fix**: Add at least one tenant:
```yaml
tenants:
  - name: mytenant
    tenantId: "..."
    clientId: "..."
    secretKey: "..."
```

### "Received error response to API login"

**Causes:**
- Invalid credentials (tenant ID, client ID, or secret)
- Expired client secret
- Missing API permissions
- Admin consent not granted

**Fix:**
1. Verify credentials in Azure Portal
2. Check client secret hasn't expired
3. Verify API permissions (ActivityFeed.Read, etc.)
4. Ensure admin consent was granted

### High Memory Usage

**Cause**: `cacheSize` too large for number of tenants.

**Fix**: Reduce `cacheSize` in config:
```yaml
collect:
  cacheSize: 100000  # Lower value
```

### Logs Not Appearing in Output

**Checks:**
1. Verify output configuration (address, port)
2. Check network connectivity to output destination
3. Review collector logs for errors
4. Ensure Office 365 is generating logs (test with Azure Portal)
5. Check `hoursToCollect` - may be looking at wrong time window

### Rate Limiting / 429 Errors

**Expected behavior**:
- Collector automatically backs off for 30 seconds
- Continues after backoff period

**If persistent:**
- Reduce `maxThreads` to lower request rate
- Increase `hoursToCollect` to reduce frequency of runs

## Security Best Practices

### Protect Configuration File

```bash
# Set restrictive permissions
chmod 600 config.yaml
chown collector-user:collector-group config.yaml
```

### Client Secret Rotation

1. Create a new secret in Azure Portal
2. Update `config.yaml` with new secret
3. Test collection
4. Delete old secret after verification

### Network Security

- All Office 365 API communication is over HTTPS
- Consider firewall rules to limit outbound connections:
  - `login.microsoftonline.com` (port 443)
  - `manage.office.com` (port 443)

## Advanced Usage

### Interactive Mode (Testing)

Test your configuration with the interactive TUI:

```bash
./office_audit_log_collector --config config.yaml --interactive
```

**Features:**
- Test API connectivity
- View log collection in real-time
- Monitor performance metrics
- Debug configuration issues

**Note**: Interactive mode uses the first tenant from config.

### Custom Filters

Filter logs before outputting:

```yaml
collect:
  filter:
    Audit.Exchange:
      Operation: "MailItemsAccessed"
      ResultStatus: "Succeeded"

    Audit.SharePoint:
      Operation: "FileDownloaded"
```

Only logs matching **ALL** filter fields are collected.

### Content Type Selection

Enable only the log types you need:

```yaml
collect:
  contentTypes:
    Audit.General: False           # Disable general logs
    Audit.AzureActiveDirectory: True
    Audit.Exchange: True
    Audit.SharePoint: True
    DLP.All: False                 # Disable DLP logs
```

## Architecture

```
┌─────────────┐
│   Config    │
│  (tenants)  │
└──────┬──────┘
       │
       v
┌──────────────────┐
│ MultiTenantCollector│
└────┬─────────────┘
     │
     ├─> Tenant 1 ─> Collector ─> API ─> Office 365
     ├─> Tenant 2 ─> Collector ─> API ─> Office 365
     └─> Tenant N ─> Collector ─> API ─> Office 365
                         │
                         v
                  ┌─────────────┐
                  │   Filters   │
                  │ Dedup Cache │
                  └──────┬──────┘
                         │
                         v
                  ┌─────────────┐
                  │   Outputs   │
                  │  (Graylog,  │
                  │  Fluentd,   │
                  │    CSV)     │
                  └─────────────┘
```

- **Concurrent**: All tenants collect simultaneously
- **Isolated**: Each tenant has independent auth, rate limits, deduplication
- **Async**: Tokio runtime handles all I/O efficiently
- **Batched**: Logs cached and sent in batches

## Documentation

- **Usage Guide**: See [MULTI_TENANT_USAGE.md](MULTI_TENANT_USAGE.md) for detailed documentation
- **Architecture**: See [MULTI_TENANT_DESIGN.md](MULTI_TENANT_DESIGN.md) for technical design
- **Examples**: Check `Release/ConfigExamples/` directory

## Support

- **Issues**: Report bugs via GitHub Issues
- **Examples**: See `Release/ConfigExamples/` for complete configurations

## License

MIT License - see LICENSE file for details

## Credits

Rebuilt from ground up based on the original project concept.
