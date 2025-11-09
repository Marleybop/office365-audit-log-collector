use std::sync::Arc;
use clap::Parser;
use crate::collector::Collector;
use crate::config::Config;
use log::{error, info, Level, LevelFilter, Log, Metadata, Record};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Mutex;
use crate::data_structures::{RunState, TenantContext};
use crate::interactive_mode::interactive;
use crate::multi_tenant_collector::MultiTenantCollector;

mod collector;
mod api_connection;
mod data_structures;
mod config;
mod interfaces;
mod interactive_mode;
mod multi_tenant_collector;


#[tokio::main]
async fn main() {

    let args = data_structures::CliArgs::parse();
    let config = Config::new(args.config.clone());
    let (log_tx, log_rx) = unbounded_channel();

    if args.interactive {
        init_interactive_logging(&config, log_tx);
        // Interactive mode requires single tenant from CLI
        let tenant = create_tenant_from_cli(&args);
        interactive::run(tenant, config, log_rx).await.unwrap();
    } else {
        init_non_interactive_logging(&config);

        // Determine if multi-tenant or single-tenant mode
        if config.tenants.is_some() {
            // Multi-tenant mode: tenants defined in config
            info!("Running in MULTI-TENANT mode");
            let multi_collector = MultiTenantCollector::new(config, args.oms_key.clone());
            multi_collector.run().await;
        } else {
            // Single-tenant mode: tenant credentials from CLI args
            info!("Running in SINGLE-TENANT mode (legacy)");
            let tenant = create_tenant_from_cli(&args);
            let state = RunState::default();
            let wrapped_state = Arc::new(Mutex::new(state));
            let runs = config.get_needed_runs();
            match Collector::new(
                tenant,
                config,
                runs,
                wrapped_state.clone(),
                args.oms_key.clone(),
                false,
                None
            ).await {
                Ok(mut collector) => collector.monitor().await,
                Err(e) => {
                    error!("Could not start collector: {}", e);
                    panic!("Could not start collector: {}", e);
                }
            }
        }
    }
}

fn create_tenant_from_cli(args: &data_structures::CliArgs) -> TenantContext {
    let tenant_id = args.tenant_id.as_ref()
        .expect("--tenant-id required in single-tenant mode")
        .clone();
    let client_id = args.client_id.as_ref()
        .expect("--client-id required in single-tenant mode")
        .clone();
    let secret_key = args.secret_key.as_ref()
        .expect("--secret-key required in single-tenant mode")
        .clone();
    let publisher_id = args.publisher_id.as_ref()
        .unwrap_or(&tenant_id)
        .clone();

    TenantContext {
        name: "default".to_string(),
        tenant_id,
        client_id,
        secret_key,
        publisher_id,
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

