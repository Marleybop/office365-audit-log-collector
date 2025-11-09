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

    // Validate that tenants are configured
    if config.tenants.is_none() || config.tenants.as_ref().unwrap().is_empty() {
        error!("No tenants configured in config file. Add a 'tenants:' section with at least one tenant.");
        panic!("No tenants configured. See config examples in Release/ConfigExamples/");
    }

    let (log_tx, log_rx) = unbounded_channel();

    if args.interactive {
        init_interactive_logging(&config, log_tx);
        // Interactive mode uses the first tenant from config
        let tenant = get_first_tenant(&config);
        info!("Interactive mode using tenant: {}", tenant.name);
        interactive::run(tenant, config, log_rx).await.unwrap();
    } else {
        init_non_interactive_logging(&config);

        let tenant_count = config.tenants.as_ref().unwrap().len();
        if tenant_count == 1 {
            info!("Starting collection for 1 tenant");
        } else {
            info!("Starting multi-tenant collection for {} tenants", tenant_count);
        }

        let multi_collector = MultiTenantCollector::new(config, args.oms_key.clone());
        multi_collector.run().await;
    }
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

