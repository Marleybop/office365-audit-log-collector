use anyhow::Result;
use chrono::{DateTime, Utc};
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use log::{error, info};

/// Collection result for a single tenant
#[derive(Debug, Clone)]
pub struct TenantResult {
    pub tenant_name: String,
    pub success: bool,
    pub logs_collected: usize,
    pub error_message: Option<String>,
}

/// Overall collection run result
#[derive(Debug, Clone)]
pub struct CollectionResult {
    pub tenant_results: Vec<TenantResult>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub total_logs: usize,
    pub success: bool,
}

impl CollectionResult {
    pub fn all_successful(&self) -> bool {
        self.tenant_results.iter().all(|r| r.success)
    }

    pub fn any_failures(&self) -> bool {
        self.tenant_results.iter().any(|r| !r.success)
    }
}

/// Notification trigger settings
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationTrigger {
    Success,
    Failure,
}

/// Email notifier
pub struct EmailNotifier {
    smtp_host: String,
    smtp_port: u16,
    username: String,
    password: String,
    from: String,
    to: String,
    triggers: Vec<NotificationTrigger>,
}

impl EmailNotifier {
    pub fn new(
        smtp_host: String,
        smtp_port: u16,
        username: String,
        password: String,
        from: String,
        to: String,
        triggers: Vec<NotificationTrigger>,
    ) -> Self {
        Self {
            smtp_host,
            smtp_port,
            username,
            password,
            from,
            to,
            triggers,
        }
    }

    /// Check if notification should be sent based on result
    pub fn should_notify(&self, result: &CollectionResult) -> bool {
        if result.all_successful() {
            self.triggers.contains(&NotificationTrigger::Success)
        } else {
            self.triggers.contains(&NotificationTrigger::Failure)
        }
    }

    /// Send notification email
    pub async fn send_notification(&self, result: &CollectionResult, next_run: DateTime<Utc>) -> Result<()> {
        if !self.should_notify(result) {
            return Ok(());
        }

        let subject = if result.all_successful() {
            "✓ Office 365 Log Collection Successful"
        } else {
            "❌ Office 365 Log Collection Failed"
        };

        let body = self.format_email_body(result, next_run);

        info!("Sending email notification to {}", self.to);

        let email = Message::builder()
            .from(self.from.parse()?)
            .to(self.to.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)?;

        let creds = Credentials::new(self.username.clone(), self.password.clone());

        let mailer = SmtpTransport::relay(&self.smtp_host)?
            .port(self.smtp_port)
            .credentials(creds)
            .build();

        match mailer.send(&email) {
            Ok(_) => {
                info!("✓ Email notification sent successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to send email notification: {}", e);
                Err(e.into())
            }
        }
    }

    /// Format email body
    fn format_email_body(&self, result: &CollectionResult, next_run: DateTime<Utc>) -> String {
        let mut body = String::new();

        body.push_str(&format!(
            "Collection completed at {}\n\n",
            result.end_time.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        body.push_str("Results:\n");
        for tenant_result in &result.tenant_results {
            if tenant_result.success {
                body.push_str(&format!(
                    "✓ {}: Collected {} new logs\n",
                    tenant_result.tenant_name, tenant_result.logs_collected
                ));
            } else {
                body.push_str(&format!(
                    "❌ {}: {}\n",
                    tenant_result.tenant_name,
                    tenant_result
                        .error_message
                        .as_ref()
                        .unwrap_or(&"Unknown error".to_string())
                ));
            }
        }

        body.push_str(&format!("\nTotal: {} logs sent to output\n", result.total_logs));

        if result.any_failures() {
            body.push_str("\nAction required: Check failed tenant credentials and configuration\n");
        }

        body.push_str(&format!(
            "Next run: {}\n",
            next_run.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_notify() {
        let notifier = EmailNotifier::new(
            "smtp.test.com".to_string(),
            587,
            "user".to_string(),
            "pass".to_string(),
            "from@test.com".to_string(),
            "to@test.com".to_string(),
            vec![NotificationTrigger::Failure],
        );

        let success_result = CollectionResult {
            tenant_results: vec![TenantResult {
                tenant_name: "test".to_string(),
                success: true,
                logs_collected: 100,
                error_message: None,
            }],
            start_time: Utc::now(),
            end_time: Utc::now(),
            total_logs: 100,
            success: true,
        };

        // Should not notify on success when only failure trigger is set
        assert!(!notifier.should_notify(&success_result));

        let failure_result = CollectionResult {
            tenant_results: vec![TenantResult {
                tenant_name: "test".to_string(),
                success: false,
                logs_collected: 0,
                error_message: Some("Auth failed".to_string()),
            }],
            start_time: Utc::now(),
            end_time: Utc::now(),
            total_logs: 0,
            success: false,
        };

        // Should notify on failure
        assert!(notifier.should_notify(&failure_result));
    }
}
