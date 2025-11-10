# Running with Docker

Complete guide for building and running the Office 365 Audit Log Collector in a Docker container.

## Quick Start

```bash
# 1. Create your config file
cp config.minimal.yaml config.yaml
# Edit config.yaml with your tenant credentials and schedule

# 2. Create data directory
mkdir -p data

# 3. Build and run in daemon mode
docker-compose up -d

# 4. View logs
docker-compose logs -f
```

The container runs continuously in daemon mode, collecting logs according to your schedule.

## Detailed Instructions

### Step 1: Prepare Configuration

Create your `config.yaml` file:

```bash
# Copy the minimal example (recommended for beginners)
cp config.minimal.yaml config.yaml

# Or copy the advanced example
# cp config.advanced.yaml config.yaml

# Edit with your favorite editor
nano config.yaml  # or vim, or any editor
```

Update these required values:
- `tenantId`: Your Azure AD tenant ID
- `clientId`: Your App Registration client ID
- `secretKey`: Your App Registration client secret
- `schedule.interval`: How often to collect (e.g., `3h`)

Optional but recommended:
- `notifications.email`: SMTP settings for email alerts

See [README.md](README.md) for Azure App Registration setup instructions.

### Step 2: Build the Container

#### Option A: Using docker-compose (Recommended)

```bash
docker-compose build
```

#### Option B: Using docker directly

```bash
docker build -t office365-audit-collector:latest .
```

**Build time:** ~5-10 minutes (first build), ~30 seconds (subsequent builds with cache)

### Step 3: Run the Container

#### Option A: Using docker-compose (Recommended)

```bash
# Start in background
docker-compose up -d

# View logs
docker-compose logs -f

# Stop
docker-compose down
```

#### Option B: Using docker run

```bash
# Create data directory
mkdir -p data

# Run container
docker run -d \
  --name office365-collector \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v $(pwd)/data:/app/data \
  --restart unless-stopped \
  office365-audit-collector:latest
```

### Step 4: Verify It's Working

```bash
# Check container is running
docker ps

# View logs
docker logs -f office365-collector

# You should see:
# "Starting Office 365 Audit Log Collector v2.5.0"
# "Daemon mode enabled"
# "Configured tenants: 1" (or more)
# "Schedule: 3h"
# "Running initial collection..."
# "Starting multi-tenant collection for X tenants"
# "Collection complete: X total logs"
# "Next collection scheduled for: ..."
```

The container will:
1. Collect immediately on startup
2. Wait for the next scheduled run
3. Continue running indefinitely

## Advanced Usage

### Interactive Mode (Testing)

Test your configuration interactively:

```bash
docker run -it --rm \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v $(pwd)/data:/app/data \
  office365-audit-collector:latest \
  --config /app/config.yaml --interactive
```

### Azure Log Analytics

If using Azure Log Analytics output:

```bash
docker run -d \
  --name office365-collector \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v $(pwd)/data:/app/data \
  office365-audit-collector:latest \
  --config /app/config.yaml --oms-key "YOUR-OMS-KEY-HERE"
```

Or with docker-compose, edit the `command:` line:

```yaml
services:
  office365-collector:
    command: ["--config", "/app/config.yaml", "--oms-key", "YOUR-OMS-KEY"]
```

### Running with Graylog Stack

The `docker-compose.yml` includes a commented-out Graylog stack. To use it:

1. Uncomment the Graylog services section
2. Update `config.yaml` to use `graylog` as the address:
   ```yaml
   output:
     graylog:
       address: graylog  # Docker service name
       port: 5555
   ```
3. Start everything:
   ```bash
   docker-compose up -d
   ```
4. Access Graylog at http://localhost:9000 (admin/admin)

## Directory Structure

```
office365-audit-log-collector/
├── config.yaml              # Your configuration (DO NOT COMMIT)
├── config.example.yaml      # Example configuration
├── data/                    # Persistent data
│   ├── known_blobs_tenant1  # Deduplication files
│   ├── known_blobs_tenant2
│   └── collector.log        # Optional log file
├── docker-compose.yml       # Docker Compose configuration
├── Dockerfile              # Container build instructions
└── ...
```

## Volumes Explained

### Config Volume (Read-Only)

```bash
-v $(pwd)/config.yaml:/app/config.yaml:ro
```

- **Purpose**: Mounts your config file into the container
- **Read-only**: `:ro` flag prevents container from modifying it
- **Location**: `/app/config.yaml` inside container

### Data Volume (Read-Write)

```bash
-v $(pwd)/data:/app/data
```

- **Purpose**: Persistent storage for deduplication files
- **Contains**:
  - `known_blobs_*` files (one per tenant)
  - Optional log files
  - Optional CSV output files
- **Persists**: Data survives container restarts/removals

## Scheduling with Docker

### Built-in Daemon Mode (Default)

The container has a built-in scheduler - no external cron needed!

Configure the schedule in `config.yaml`:

```yaml
schedule:
  interval: 3h  # Options: 30m, 1h, 3h, 6h, 12h, 1d
  # OR use cron:
  # cron: "0 */3 * * *"  # Every 3 hours
```

Then run with docker-compose:

```bash
docker-compose up -d
```

The container will:
- Run collection immediately on startup
- Continue running indefinitely
- Collect on schedule
- Send email notifications (if configured)
- Restart automatically (`restart: unless-stopped`)

### Manual Trigger (--run-now)

Run collection manually without waiting for schedule:

```bash
# Run once and exit
docker exec office365-collector \
  /app/office_audit_log_collector --config /app/config.yaml --run-now

# Check exit code
echo $?  # 0 = success, 1 = failure
```

Use cases:
- Manual data backfill
- Testing after config changes
- Triggered by external event/webhook

### External Scheduling (Alternative)

If you prefer external scheduling over the built-in daemon:

#### Kubernetes Job (with external CronJob)

```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: office365-audit-collector
spec:
  schedule: "0 */3 * * *"  # Every 3 hours
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: collector
            image: office365-audit-collector:latest
            args: ["--config", "/app/config.yaml", "--run-now"]
            volumeMounts:
            - name: config
              mountPath: /app/config.yaml
              subPath: config.yaml
              readOnly: true
            - name: data
              mountPath: /app/data
          volumes:
          - name: config
            secret:
              secretName: office365-collector-config
          - name: data
            persistentVolumeClaim:
              claimName: collector-data
          restartPolicy: OnFailure
```

**Note:** Config doesn't need `schedule:` section when using `--run-now`.

## Troubleshooting

### Container Exits Immediately

```bash
# Check logs
docker logs office365-collector

# Common issues:
# - Missing config file
# - Invalid YAML in config
# - No tenants configured
```

### "No tenants configured" Error

Your `config.yaml` is missing the `tenants:` section. Add at least one tenant:

```yaml
tenants:
  - name: my-tenant
    tenantId: "..."
    clientId: "..."
    secretKey: "..."
```

### Permission Denied on Data Directory

```bash
# Fix permissions
sudo chown -R 1000:1000 data/

# Or run container as root (not recommended)
docker run --user root ...
```

### Can't Connect to Graylog

If using `localhost` or `127.0.0.1` in config:
- From inside Docker, `localhost` refers to the container itself
- Use the Graylog service name: `graylog`
- Or use host IP: `host.docker.internal` (Mac/Windows) or actual IP (Linux)

### Config Changes Not Taking Effect

```bash
# Restart the container
docker-compose restart

# Or with docker:
docker restart office365-collector
```

### High Memory Usage

Reduce `cacheSize` in config:

```yaml
collect:
  cacheSize: 100000  # Lower value for Docker
```

## Building for Different Architectures

### ARM64 (Apple M1/M2, Raspberry Pi)

```bash
docker buildx build --platform linux/arm64 -t office365-audit-collector:arm64 .
```

### Multi-arch Build

```bash
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t your-registry/office365-audit-collector:latest \
  --push .
```

## Security Best Practices

### Protect Your Config

```bash
# Set restrictive permissions
chmod 600 config.yaml

# Never commit it to git
echo "config.yaml" >> .gitignore
```

### Use Docker Secrets (Swarm/K8s)

Instead of mounting config file, use secrets:

```bash
# Create secret
docker secret create collector-config config.yaml

# Use in service
docker service create \
  --name office365-collector \
  --secret collector-config \
  office365-audit-collector:latest
```

### Run as Non-Root (Default)

The Dockerfile already creates and uses a non-root user (`collector`, UID 1000).

## Updating the Container

```bash
# Pull latest code
git pull

# Rebuild
docker-compose build

# Restart
docker-compose up -d
```

## Cleanup

```bash
# Stop and remove container
docker-compose down

# Remove container and volumes
docker-compose down -v

# Remove image
docker rmi office365-audit-collector:latest
```

## Support

For issues specific to Docker setup, check:
- Docker logs: `docker logs office365-collector`
- Container status: `docker ps -a`
- Resource usage: `docker stats office365-collector`

For application issues, see [README.md](README.md) troubleshooting section.
