use anyhow::{Context, Result};
use lettre::{
    transport::smtp::authentication::Credentials,
    transport::smtp::client::{TlsParameters, Tls},
    Message, SmtpTransport, Transport,
};
use lettre::message::Mailbox;
use log::info;
use serde::{Deserialize, Serialize};

use crate::backend::domain::models::transaction::Transaction;
use crate::backend::domain::models::child::Child;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub to_emails: Vec<String>, // Changed from single to_email to list of emails
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_server: "smtp.gmail.com".to_string(),
            smtp_port: 587,
            username: String::new(),
            password: String::new(),
            from_email: String::new(),
            to_emails: Vec::new(),
        }
    }
}

pub struct EmailService {
    config: EmailConfig,
    transport: Option<SmtpTransport>,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            config,
            transport: None,
        }
    }

    pub fn initialize(&mut self) -> Result<()> {
        info!("📧 Initializing email service for SMTP server: {}:{}", self.config.smtp_server, self.config.smtp_port);
        
        let tls_params = TlsParameters::new(self.config.smtp_server.clone())
            .context("Failed to create TLS parameters")?;

        let transport = SmtpTransport::relay(&self.config.smtp_server)
            .context("Failed to create SMTP relay")?
            .port(self.config.smtp_port)
            .tls(Tls::Required(tls_params))
            .credentials(Credentials::new(
                self.config.username.clone(),
                self.config.password.clone(),
            ))
            .build();

        self.transport = Some(transport);
        info!("📧 Email service initialized successfully");
        Ok(())
    }

    pub fn send_transaction_notification(
        &self,
        transaction: &Transaction,
        child: &Child,
        action: &str,
        current_balance: f64,
    ) -> Result<()> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Email service not initialized"))?;

        let subject = format!(
            "Allowance Tracker - {} {} ${:.2}",
            child.name, action, transaction.amount.abs()
        );

        let body = format!(
            "Hello!\n\n{} has {} ${:.2}.\n\nTransaction Details:\n- Amount: ${:.2}\n- Description: {}\n- Date: {}\n\nCurrent Balance: ${:.2}\n\nBest regards,\nAllowance Tracker",
            child.name,
            action,
            transaction.amount.abs(),
            transaction.amount.abs(),
            transaction.description,
            transaction.date.format("%B %d, %Y"),
            current_balance
        );

        // Build email with BCC for multiple recipients
        let mut email_builder = Message::builder()
            .from(self
                .config
                .from_email
                .parse::<Mailbox>()
                .context("Failed to parse from email")?);

        // Add BCC recipients if any are configured
        if !self.config.to_emails.is_empty() {
            for email in &self.config.to_emails {
                email_builder = email_builder.bcc(email.parse::<Mailbox>().context("Failed to parse BCC email")?);
            }
        } else {
            info!("📧 No email recipients configured, skipping email send");
            return Ok(());
        }

        let email = email_builder
            .subject(subject)
            .body(body)
            .context("Failed to build email")?;

        transport.send(&email).context("Failed to send email")?;
        info!("📧 Transaction notification email sent successfully to {} recipients", self.config.to_emails.len());
        Ok(())
    }

    pub fn send_transaction_deleted_notification(
        &self,
        transaction: &Transaction,
        child: &Child,
        current_balance: f64,
    ) -> Result<()> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Email service not initialized"))?;

        let subject = format!(
            "Allowance Tracker - Transaction Deleted for {}",
            child.name
        );

        let body = format!(
            "Hello!\n\nA transaction for {} has been deleted.\n\nDeleted Transaction Details:\n- Amount: ${:.2}\n- Description: {}\n- Date: {}\n\nCurrent Balance: ${:.2}\n\nBest regards,\nAllowance Tracker",
            child.name,
            transaction.amount,
            transaction.description,
            transaction.date.format("%B %d, %Y"),
            current_balance
        );

        // Build email with BCC for multiple recipients
        let mut email_builder = Message::builder()
            .from(self
                .config
                .from_email
                .parse::<Mailbox>()
                .context("Failed to parse from email")?);

        // Add BCC recipients if any are configured
        if !self.config.to_emails.is_empty() {
            for email in &self.config.to_emails {
                email_builder = email_builder.bcc(email.parse::<Mailbox>().context("Failed to parse BCC email")?);
            }
        } else {
            info!("📧 No email recipients configured, skipping email send");
            return Ok(());
        }

        let email = email_builder
            .subject(subject)
            .body(body)
            .context("Failed to build email")?;

        transport.send(&email).context("Failed to send email")?;
        info!("📧 Transaction deletion notification email sent successfully to {} recipients", self.config.to_emails.len());
        Ok(())
    }
}

// Thread-safe wrapper for the email service
// #[derive(Clone)]
pub struct EmailServiceWrapper {
    service: EmailService,
}

impl EmailServiceWrapper {
    pub fn new(config: EmailConfig) -> Result<Self> {
        let mut service = EmailService::new(config);
        service.initialize()?;
        Ok(Self {
            service,
        })
    }

    pub fn send_transaction_notification(
        &self,
        transaction: &Transaction,
        child: &Child,
        action: &str,
        current_balance: f64,
    ) -> Result<()> {
        self.service.send_transaction_notification(transaction, child, action, current_balance)
    }

    pub fn send_transaction_deleted_notification(
        &self,
        transaction: &Transaction,
        child: &Child,
        current_balance: f64,
    ) -> Result<()> {
        self.service.send_transaction_deleted_notification(transaction, child, current_balance)
    }
} 