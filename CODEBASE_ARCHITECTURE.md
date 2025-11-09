# Office 365 Audit Log Collector - Codebase Architecture Summary

## Project Overview
- **Language**: Rust (recently rewritten from Python for stability)
- **Version**: 2.5.0
- **Type**: Stand-alone audit log collection utility
- **Purpose**: Collect Office 365 audit logs from multiple content types and send to various output backends

## Overall Project Structure

```
office365-audit-log-collector/
├── src/
│   ├── main.rs                          # Entry point, logging setup
│   ├── api_connection.rs                # Office 365 API interaction (500+ lines)
│   ├── collector.rs                     # Main collection orchestration logic
│   ├── config.rs                        # Configuration file parsing (YAML)
│   ├── data_structures.rs               # Core data types and CLI args
│   ├── interfaces/                      # Output backend implementations
│   │   ├── interface.rs                 # Interface trait definition
│   │   ├── graylog_interface.rs         # TCP/Socket output to Graylog
│   │   ├── fluentd_interface.rs         # Fluentd output integration
│   │   ├── file_interface.rs            # CSV file output
│   │   ├── azure_oms_interface.rs       # Azure Log Analytics output
│   │   └── interactive_interface.rs     # TUI-based output
│   └── interactive_mode/                # Terminal UI for testing/debugging
│       ├── interactive.rs               # Interactive mode main logic
│       ├── tui.rs                       # Terminal UI components
│       └── mod.rs
├── Cargo.toml                           # Rust dependencies
├── Release/
│   ├── Dockerfile                       # Container definition
│   ├── Linux/                           # Pre-built Linux binaries
│   ├── Windows/                         # Pre-built Windows binaries
│   └── ConfigExamples/                  # Sample YAML configurations
└── README.md                            # Documentation
```

## Authentication Flow

### Current Authentication (Single-Tenant Only)

The system uses **Client Credentials OAuth 2.0 flow** for Azure AD:

```
1. CLI Arguments (required):
   - tenant_id: Azure AD tenant identifier
   - client_id: Application (Client) ID from App Registration
   - secret_key: Client secret value
   - config: Path to YAML configuration file

2. Login Process (api_connection.rs::login()):
   - POST to: https://login.microsoftonline.com/{tenant_id}/oauth2/token
   - Body: grant_type=client_credentials, client_id, client_secret, resource=https://manage.office.com
   - Response: Bearer token (access_token)
   - Token stored in HeaderMap for subsequent API requests

3. API Base URL Pattern:
   https://manage.office.com/api/v1.0/{tenant_id}/activity/feed
```

### Authentication Limitations (Single-Tenant):
- Only ONE tenant_id can be used per execution
- One set of credentials (client_id + secret_key) required per tenant
- No built-in loop to handle multiple tenants
- Would require external orchestration (cron, Kubernetes) to collect from multiple tenants

## Audit Log Collection Process

### Collection Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ 1. MAIN THREAD                                              │
│    - Parses CLI args and config                             │
│    - Initializes all output interfaces                      │
│    - Spawns three background threads                        │
└─────────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ 2. BLOB THREAD  │ │ 3. CONTENT THRD │ │ 4. MSG LOOP THR │
│ (find blobs)    │ │ (retrieve cont) │ │ (orchestrate)   │
└─────────────────┘ └─────────────────┘ └─────────────────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            │
                            ▼
                    ┌─────────────────┐
                    │ 5. MAIN MONITOR │
                    │ - Filters logs  │
                    │ - Caches logs   │
                    │ - Sends output  │
                    └─────────────────┘
```

### Collection Steps in Detail

1. **Subscription Setup** (api_connection.rs::subscribe_to_feeds)
   - Get current feed subscriptions from Office API
   - Subscribe to enabled content types if not already subscribed
   - Content types: Audit.General, Audit.AzureActiveDirectory, Audit.Exchange, Audit.SharePoint, DLP.All

2. **Create Base URLs** (api_connection.rs::create_base_urls)
   - For each content type and time range (split into max 24-hour chunks)
   - Generate URLs for: `/subscriptions/content?contentType=X&startTime=Y&endTime=Z&PublisherIdentifier=ID`
   - Max lookback: 168 hours (7 days) per Office API limit
   - Time ranges split into 24-hour windows automatically

3. **Retrieve Content Blobs** (api_connection.rs::get_content_blobs)
   - Spawned as async task with multi-threaded tokio runtime
   - Max concurrent threads: configurable (default 50)
   - For each URL, handle pagination via `NextPageUri` header
   - Store found content IDs/URIs to retrieve next
   - Track failures for retry mechanism (default 3 retries)

4. **Retrieve Actual Content** (api_connection.rs::get_content)
   - Download individual content from contentURI
   - Content is JSON array of audit log events
   - Handle timeouts (3 second timeout per request)
   - Track rate limiting (429 errors trigger 30-second backoff)

5. **Filtering & Caching** (collector.rs::handle_log)
   - Parse JSON response
   - Apply optional filters per content type
   - Skip already-processed logs (via known_blobs file)
   - Add "OriginFeed" field for tracking source
   - Cache in memory until batch size reached

6. **Output Batching** (collector.rs::output)
   - Cache size: configurable (default 500,000 logs)
   - Flush to all enabled output interfaces when cache full
   - Interfaces can be: Graylog, Fluentd, CSV, Azure Log Analytics

### Configuration Options for Collection

```yaml
collect:
  workingDir: ./                    # Directory for cache files (known_blobs)
  contentTypes:
    Audit.General: true             # Which audit logs to collect
    Audit.AzureActiveDirectory: true
    Audit.Exchange: true
    Audit.SharePoint: true
    DLP.All: true
  cacheSize: 500000                 # Logs to batch before output
  maxThreads: 50                    # Concurrent API requests
  globalTimeout: 1                  # Minutes before forced exit
  retries: 3                        # Retry failed blobs
  skipKnownLogs: true               # Track processed blobs
  hoursToCollect: 24                # Look-back window (max 168)
  duplicate: 1                      # Load test: duplicate each log N times
  filter:                           # Optional content-type-specific filters
    Audit.General:
      Operation: "UserAdminActivity"  # Only logs matching ALL filters
```

## Output Interfaces

All interfaces implement the async `Interface` trait with `send_logs(logs: Caches)` method:

### 1. Graylog Interface (graylog_interface.rs)
- **Purpose**: Send audit logs to Graylog over TCP
- **Protocol**: Raw JSON over TCP socket
- **Configuration**:
  ```yaml
  output:
    graylog:
      address: localhost
      port: 5555
  ```
- **Processing**:
  - Parses "CreationTime" field from log
  - Converts to "timestamp" field in `YYYY-MM-DD HH:MM:SS.mmm` format
  - Sends each log as separate JSON object over TCP
  - Creates new socket connection per flush (not persistent)

### 2. Fluentd Interface (fluentd_interface.rs)
- **Purpose**: Send logs to Fluentd/Fluent Bit
- **Protocol**: Fluentd protocol via poston library
- **Configuration**:
  ```yaml
  output:
    fluentd:
      tenantName: MyOrganization     # Used as tag in Fluentd
      address: localhost
      port: 24224
  ```
- **Processing**:
  - Uses poston WorkerPool for connection management
  - Batches: 1000 logs max, 10ms flush period
  - Tag format: uses tenantName from config
  - Extracts CreationTime as System time

### 3. File (CSV) Interface (file_interface.rs)
- **Purpose**: Export audit logs to CSV files
- **Configuration**:
  ```yaml
  output:
    file:
      path: 'output.csv'
      separateByContentType: true    # Create one CSV per content type
      separator: ';'
  ```
- **Processing**:
  - Generates dynamic columns from JSON keys
  - Can output all logs to single file or separate by content type
  - Filenames include timestamp: `{timestamp}_{filename}_{ContentType}.csv`
  - Pads missing fields with empty strings

### 4. Azure Log Analytics (OMS) Interface (azure_oms_interface.rs)
- **Purpose**: Send logs to Azure Log Analytics workspace
- **Protocol**: HTTP POST with HMAC-SHA256 signature
- **Configuration**:
  ```yaml
  output:
    azureLogAnalytics:
      workspaceId: "workspace-uuid"  # Requires --oms-key CLI arg
  ```
- **Processing**:
  - Requires Azure Log Analytics workspace shared key (passed via --oms-key)
  - Builds HMAC-SHA256 signature per request
  - HTTP endpoint: `https://{workspaceId}.ods.opinsights.azure.com/api/logs`
  - Table name derived from content type: Audit_Exchange, Audit_SharePoint, etc.
  - Sends 10 concurrent requests (buffer_unordered)

### 5. Interactive Interface (for testing)
- **Purpose**: Display logs in terminal UI during interactive testing
- **Usage**: Only when running with `--interactive` flag
- **Features**: 
  - Real-time log preview in terminal
  - Test API connectivity
  - Load testing support

## Configuration System

### Config File Format (YAML)
```yaml
log:                              # Optional logging configuration
  path: 'collector.log'           # Log file path
  debug: false                    # Enable debug logging

collect:                          # Core collection settings
  # (see above section)

output:                           # Output interface(s) - can have multiple
  # (see above section)
```

### Known Blobs Tracking
- **File**: `{workingDir}/known_blobs`
- **Format**: CSV-like: `contentId,creationTime`
- **Purpose**: Avoid re-processing same audit logs in subsequent runs
- **Expiration**: Automatically cleaned when log expires (based on creationTime)

### Configuration Loading (config.rs)
- Reads YAML file using serde_yaml
- Validates content types and output options
- Splits time ranges > 24 hours automatically
- Saves/loads known_blobs for deduplication

## Multi-Tenant Status & Limitations

### Current State: SINGLE-TENANT ONLY
The codebase is fundamentally single-tenant by design:

1. **Single Credential Set per Run**
   - One `--tenant-id`, `--client-id`, `--secret-key` per execution
   - API connection is tenant-specific in constructor

2. **No Built-in Multi-Tenant Loop**
   - Main.rs directly creates one Collector instance
   - No outer loop to iterate over multiple tenants

3. **Fluentd "tenant-aware" Output**
   - Has `tenantName` config field (e.g., "MyOrganization")
   - Used as Fluentd tag for routing/filtering on receiver
   - **But**: This is just a label, not enabling true multi-tenancy
   - All logs from one run go to one tenant name

### What Would Be Required for Multi-Tenant:

1. **Configuration Level**:
   - Config file should support array of tenant credentials
   - Each tenant should have its own output routing rules
   - Would need to restructure output config

2. **Orchestration Level**:
   - Loop over multiple tenants in main() or wrapper script
   - Create separate Collector per tenant
   - Potential parallel execution with thread pool

3. **Code Changes**:
   - Modify CliArgs to support multiple credentials OR
   - Create wrapper script that invokes binary multiple times with different args
   - Update interfaces to route logs by tenant
   - Azure OMS workspace might need per-tenant workspaceId

## Key Files Reference

| File | Lines | Purpose |
|------|-------|---------|
| main.rs | 100 | Entry point, logging setup |
| api_connection.rs | 500+ | Office 365 API calls, blob/content retrieval |
| collector.rs | 500+ | Main orchestration, filtering, output routing |
| config.rs | 260 | YAML config parsing, known_blobs management |
| data_structures.rs | 190 | Data types, CLI args definition |
| graylog_interface.rs | 113 | Graylog TCP output |
| fluentd_interface.rs | 65 | Fluentd output via poston |
| file_interface.rs | 148 | CSV export |
| azure_oms_interface.rs | 130+ | Azure Log Analytics HMAC signatures |
| interactive.rs | 941 | TUI for testing/debugging |

## Dependencies (Key)

```
reqwest (HTTP client)           - Office 365 API calls
tokio (async runtime)            - Concurrent blob/content retrieval
serde_yaml (YAML parsing)        - Config file parsing
chrono (datetime)                - Timestamp handling
csv (CSV writing)                - File output
poston (Fluentd client)          - Fluentd output
ratatui (TUI)                    - Interactive mode terminal UI
clap (CLI)                       - Command-line argument parsing
hmac, sha2, base64               - Azure OMS signatures
```

## Command-Line Interface

```bash
OfficeAuditLogCollector \
  --tenant-id "11111111-1111-1111-1111-111111111111" \
  --client-id "11111111-1111-1111-1111-111111111111" \
  --secret-key "secret_value_here" \
  --config /path/to/config.yaml \
  [--publisher-id "11111111-1111-1111-1111-111111111111"] \
  [--oms-key "base64_encoded_workspace_key"] \
  [--interactive]  # Enable TUI for testing
```

## Execution Flow (Non-Interactive)

```
1. Parse CLI args
2. Load config YAML
3. Initialize logging (file or stderr)
4. Connect to Office 365 API (get bearer token)
5. Subscribe to configured audit feeds
6. Load known_blobs (processed logs)
7. Create base URLs for collection (split by 24h windows)
8. Spawn three background threads:
   - Blob retrieval (handles pagination)
   - Content retrieval (downloads JSON)
   - Message loop (orchestration & retries)
9. Monitor incoming logs:
   - Parse JSON
   - Apply filters
   - Cache in memory
   - When cache full: flush to all outputs
10. Wait for completion signal from message loop
11. Final flush of remaining cached logs
12. Save known_blobs file (for next run)
13. Exit
```

## Performance Characteristics

- **Max Concurrent Connections**: 50 (configurable)
- **Cache Size**: 500,000 logs (configurable)
- **Blob Fetch Timeout**: 5 seconds
- **Content Fetch Timeout**: 3 seconds
- **Socket Connection Timeout**: 10 seconds
- **Fluentd Batch**: 1000 logs, 10ms period
- **Azure OMS Concurrent Requests**: 10

## Rate Limiting Handling

- Office 365 API responds with 429 (Too Many Requests)
- Collector detects "too many request" in response
- Triggers 30-second backoff
- Resumes normal operation after timeout
- No automatic retry during backoff (skips but tracks)

## Onboarding Steps (from README)

1. Enable auditing in Office 365 tenant
2. Create App Registration in Azure AD
3. Create Client Secret for the app
4. Grant permissions:
   - ActivityFeed.Read
   - ActivityFeed.ReadDlp
5. Configure YAML file with outputs
6. Run binary or container with credentials

## Docker Deployment

```dockerfile
FROM debian:stable-slim
COPY Linux/OfficeAuditLogCollector /
RUN apt-get update && apt-get install ca-certificates -y
WORKDIR /app
USER 1001
ENTRYPOINT ["/OfficeAuditLogCollector"]
```

Usage:
```bash
docker run -d \
  -v /configs:/configs \
  --mount source=collector-volume,target=/app \
  ghcr.io/ddbnl/office365-audit-log-collector:release \
  --tenant-id "..." --client-id "..." --secret-key "..." \
  --config /configs/graylog.yaml
```

## Testing & Interactive Mode

Running with `--interactive` flag enables:
- Terminal UI for connection testing
- API endpoint validation
- Log preview
- Load testing (duplicate logs N times)
- Real-time troubleshooting
- No output to Graylog/Fluentd/files in this mode

## Summary of Architecture Strengths

1. **Pure Rust**: Fast, compiled, no runtime dependency issues
2. **Async/Await**: Efficient concurrent I/O for blob/content retrieval
3. **Flexible Outputs**: Multiple backends (Graylog, Fluentd, CSV, Azure)
4. **Rate-Limit Aware**: Handles Office API throttling
5. **Deduplication**: Tracks processed blobs to avoid re-sending
6. **Modular**: Interface trait allows easy addition of new outputs
7. **Docker Ready**: Container deployment supported
8. **Interactive Mode**: Built-in testing UI

## Summary of Limitations

1. **Single-Tenant**: No native multi-tenant support in code
2. **No External State**: Can't coordinate with other collectors
3. **Config File Static**: No dynamic configuration reloading
4. **Fluentd Tag Static**: Can't route by tenant at output level currently
5. **No Built-in Scheduling**: Requires external orchestration (cron/Kubernetes)
6. **Socket-Based Graylog**: Not using GELF over HTTP
7. **OAuth Only**: No support for other auth methods (certificate-based, etc.)

