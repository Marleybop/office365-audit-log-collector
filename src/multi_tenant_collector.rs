use std::sync::Arc;
use chrono::Utc;
use log::info;
use tokio::sync::Mutex;
use crate::collector::Collector;
use crate::config::Config;
use crate::data_structures::{RunState, TenantContext};
use crate::notifications::{CollectionResult, TenantResult};

/// Multi-tenant orchestrator that manages concurrent collection from multiple Office 365 tenants
pub struct MultiTenantCollector {
    config: Config,
    oms_key: String,
}

impl MultiTenantCollector {

    pub fn new(config: Config, oms_key: String) -> Self {
        MultiTenantCollector {
            config,
            oms_key,
        }
    }

    /// Run collection for all configured tenants concurrently
    pub async fn run(&self) -> CollectionResult {
        let start_time = Utc::now();
        let tenants = self.config.tenants.as_ref()
            .expect("No tenants configured in config file");

        info!("Starting multi-tenant collection for {} tenants", tenants.len());

        // Create a vector to hold all tenant collector tasks
        let mut handles = Vec::new();

        for tenant_config in tenants.iter() {
            let tenant_context = TenantContext {
                name: tenant_config.name.clone(),
                tenant_id: tenant_config.tenant_id.clone(),
                client_id: tenant_config.client_id.clone(),
                secret_key: tenant_config.secret_key.clone(),
                publisher_id: tenant_config.publisher_id.as_ref()
                    .unwrap_or(&tenant_config.tenant_id)
                    .clone(),
            };

            let config = self.config.clone();
            let oms_key = self.oms_key.clone();

            // Spawn a task for each tenant
            let handle = tokio::spawn(async move {
                Self::run_tenant_collector(tenant_context, config, oms_key).await
            });

            handles.push(handle);
        }

        // Wait for all tenant collectors to complete and collect results
        let mut tenant_results = Vec::new();
        let mut total_logs = 0;

        for handle in handles {
            match handle.await {
                Ok(result) => {
                    total_logs += result.logs_collected;
                    tenant_results.push(result);
                }
                Err(e) => {
                    log::error!("Tenant collector task failed: {}", e);
                }
            }
        }

        let end_time = Utc::now();
        let success = tenant_results.iter().all(|r| r.success);

        info!("Multi-tenant collection completed for all tenants");

        CollectionResult {
            tenant_results,
            start_time,
            end_time,
            total_logs,
            success,
        }
    }

    /// Run collector for a single tenant
    async fn run_tenant_collector(tenant: TenantContext, config: Config, oms_key: String) -> TenantResult {
        info!("Starting collection for tenant: {}", tenant.name);

        let state = RunState::default();
        let wrapped_state = Arc::new(Mutex::new(state));
        let runs = config.get_needed_runs();

        match Collector::new(
            tenant.clone(),
            config,
            runs,
            wrapped_state.clone(),
            oms_key,
            false,
            None
        ).await {
            Ok(mut collector) => {
                info!("Collector initialized for tenant: {}", tenant.name);
                collector.monitor().await;
                let logs_collected = collector.logs_collected();
                info!("Collection completed for tenant: {} - {} logs collected", tenant.name, logs_collected);

                TenantResult {
                    tenant_name: tenant.name,
                    success: true,
                    logs_collected,
                    error_message: None,
                }
            },
            Err(e) => {
                let error_msg = format!("{}", e);
                log::error!("Could not start collector for tenant {}: {}", tenant.name, error_msg);

                TenantResult {
                    tenant_name: tenant.name,
                    success: false,
                    logs_collected: 0,
                    error_message: Some(error_msg),
                }
            }
        }
    }
}
