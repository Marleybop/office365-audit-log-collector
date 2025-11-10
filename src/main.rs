use clap::Parser;
use crate::config::Config;
use log::{error, info, Level, LevelFilter, Log, Metadata, Record};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use crate::data_structures::TenantContext;
use crate::interactive_mode::interactive;
use crate::multi_tenant_collector::MultiTenantCollector;
use crate::scheduler::{Scheduler, ScheduleType};
use crate::notifications::{EmailNotifier, NotificationTrigger};

mod collector;
mod api_connection;
mod data_structures;
mod config;
mod interfaces;
mod interactive_mode;
mod multi_tenant_collector;
mod scheduler;
mod notifications;


#[tokio::main]
async fn main() {

    let args = data_structures::CliArgs::parse();
    let config = Config::new(args.config.clone());

    // Validate that tenants are configured
    if config.tenants.is_none() || config.tenants.as_ref().unwrap().is_empty() {
        error!("No tenants configured in config file. Add a 'tenants:' section with at least one tenant.");
        panic!("No tenants configured. See config examples in Release/ConfigExamples/");
    }

    let (log_tx, log_rx) = unbounded_channel();

    if args.interactive {
        // Interactive mode
        init_interactive_logging(&config, log_tx);
        let tenant = get_first_tenant(&config);
        info!("Interactive mode using tenant: {}", tenant.name);
        interactive::run(tenant, config, log_rx).await.unwrap();
    } else if args.run_now {
        // Run-now mode: Execute once and exit
        init_non_interactive_logging(&config);
        info!("Running collection once (--run-now mode)");
        run_collection_once(config, args.oms_key).await;
    } else {
        // Daemon mode: Run on schedule
        init_non_interactive_logging(&config);
        run_daemon_mode(config, args.oms_key).await;
    }
}

/// Run collection once and exit
async fn run_collection_once(config: Config, oms_key: String) {
    let tenant_count = config.tenants.as_ref().unwrap().len();
    if tenant_count == 1 {
        info!("Starting collection for 1 tenant");
    } else {
        info!("Starting multi-tenant collection for {} tenants", tenant_count);
    }

    let multi_collector = MultiTenantCollector::new(config.clone(), oms_key);
    let result = multi_collector.run().await;

    // Log summary
    info!("Collection complete: {} total logs", result.total_logs);
    for tenant_result in &result.tenant_results {
        if tenant_result.success {
            info!("✓ {}: {} logs", tenant_result.tenant_name, tenant_result.logs_collected);
        } else {
            error!("✗ {}: {}", tenant_result.tenant_name,
                tenant_result.error_message.as_ref().unwrap_or(&"Unknown error".to_string()));
        }
    }

    // Send notification if configured
    if let Some(notifier) = create_email_notifier(&config) {
        let next_run = chrono::Utc::now(); // No next run in run-once mode
        if let Err(e) = notifier.send_notification(&result, next_run).await {
            error!("Failed to send notification: {}", e);
        }
    }

    // Exit with appropriate code
    if result.success {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}

/// Run in daemon mode with scheduling
async fn run_daemon_mode(config: Config, oms_key: String) {
    info!("Starting Office 365 Audit Log Collector v{}", env!("CARGO_PKG_VERSION"));

    // Parse schedule from config
    let schedule_config = config.schedule.as_ref()
        .expect("Schedule configuration required for daemon mode. Add 'schedule:' section or use --run-now");

    let schedule_str = schedule_config.interval.as_ref()
        .or(schedule_config.cron.as_ref())
        .expect("Either 'interval' or 'cron' must be specified in schedule configuration");

    let schedule_type = match ScheduleType::parse(schedule_str) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to parse schedule: {}", e);
            panic!("Invalid schedule configuration");
        }
    };

    let scheduler = Scheduler::new(schedule_type);
    let tenant_count = config.tenants.as_ref().unwrap().len();

    info!("Daemon mode enabled");
    info!("Configured tenants: {}", tenant_count);
    info!("Schedule: {}", schedule_str);

    // Create email notifier if configured
    let notifier = create_email_notifier(&config);

    // Run immediate collection on startup
    info!("Running initial collection...");
    let multi_collector = MultiTenantCollector::new(config.clone(), oms_key.clone());
    let result = multi_collector.run().await;

    log_collection_result(&result);

    // Send notification
    if let Some(ref n) = notifier {
        let next_run = scheduler.next_run_time();
        if let Err(e) = n.send_notification(&result, next_run).await {
            error!("Failed to send notification: {}", e);
        }
    }

    // Enter scheduling loop
    loop {
        scheduler.wait_for_next_run().await;

        info!("Starting scheduled collection...");
        let multi_collector = MultiTenantCollector::new(config.clone(), oms_key.clone());
        let result = multi_collector.run().await;

        log_collection_result(&result);

        // Send notification
        if let Some(ref n) = notifier {
            let next_run = scheduler.next_run_time();
            if let Err(e) = n.send_notification(&result, next_run).await {
                error!("Failed to send notification: {}", e);
            }
        }
    }
}

/// Log collection result summary
fn log_collection_result(result: &crate::notifications::CollectionResult) {
    info!("Collection complete: {} total logs", result.total_logs);
    for tenant_result in &result.tenant_results {
        if tenant_result.success {
            info!("✓ {}: {} logs", tenant_result.tenant_name, tenant_result.logs_collected);
        } else {
            error!("✗ {}: {}", tenant_result.tenant_name,
                tenant_result.error_message.as_ref().unwrap_or(&"Unknown error".to_string()));
        }
    }
}

/// Create email notifier from config
fn create_email_notifier(config: &Config) -> Option<EmailNotifier> {
    let notification_config = config.notifications.as_ref()?;
    let email_config = notification_config.email.as_ref()?;

    if !email_config.enabled {
        return None;
    }

    // Parse notification triggers
    let triggers = email_config.on.as_ref().map(|on_list| {
        on_list.iter().filter_map(|s| {
            match s.to_lowercase().as_str() {
                "success" => Some(NotificationTrigger::Success),
                "failure" => Some(NotificationTrigger::Failure),
                _ => None,
            }
        }).collect()
    }).unwrap_or_else(|| vec![NotificationTrigger::Failure]); // Default: notify on failure only

    let from = email_config.from.clone()
        .unwrap_or_else(|| email_config.smtp.username.clone());

    Some(EmailNotifier::new(
        email_config.smtp.host.clone(),
        email_config.smtp.port,
        email_config.smtp.username.clone(),
        email_config.smtp.password.clone(),
        from,
        email_config.to.clone(),
        triggers,
    ))
}

fn get_first_tenant(config: &Config) -> TenantContext {
    let tenant_config = &config.tenants.as_ref().unwrap()[0];
    TenantContext {
        name: tenant_config.name.clone(),
        tenant_id: tenant_config.tenant_id.clone(),
        client_id: tenant_config.client_id.clone(),
        secret_key: tenant_config.secret_key.clone(),
        publisher_id: tenant_config.publisher_id.as_ref()
            .unwrap_or(&tenant_config.tenant_id)
            .clone(),
    }
}

fn init_non_interactive_logging(config: &Config) {

    let (path, level) = if let Some(log_config) = &config.log {
        let level = if log_config.debug { LevelFilter::Debug } else { LevelFilter::Info };
        (log_config.path.clone(), level)
    } else {
        ("".to_string(), LevelFilter::Info)
    };

    if !path.is_empty() {
        simple_logging::log_to_file(path, level).unwrap();
    } else {
        simple_logging::log_to_stderr(level);
    }
}

fn init_interactive_logging(config: &Config, log_tx: UnboundedSender<(String, Level)>) {

    let level = if let Some(log_config) = &config.log {
        if log_config.debug { LevelFilter::Debug } else { LevelFilter::Info }
    } else {
        LevelFilter::Info
    };
    log::set_max_level(level);
    log::set_boxed_logger(InteractiveLogger::new(log_tx)).unwrap();
}


pub struct  InteractiveLogger {
    log_tx: UnboundedSender<(String, Level)>,

}
impl InteractiveLogger {
    pub fn new(log_tx: UnboundedSender<(String, Level)>) -> Box<Self> {
        Box::new(InteractiveLogger { log_tx })
    }
}
impl Log for InteractiveLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }
    fn log(&self, record: &Record) {

        let date = chrono::Utc::now().to_string();
        let msg = format!("[{}] {}:{} -- {}",
                 date,
                 record.level(),
                 record.target(),
                 record.args());
        self.log_tx.send((msg, record.level())).unwrap()
    }
    fn flush(&self) {}
}
