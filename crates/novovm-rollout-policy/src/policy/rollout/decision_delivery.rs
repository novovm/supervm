#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde_json::json;
use std::env;

#[derive(Debug)]
struct Args {
    delivery_type: String,
    endpoint: String,
    recipient: String,
    payload_json: String,
    alert_level: String,
    source: String,
    smtp_server: String,
    smtp_port: u16,
    smtp_from: String,
    smtp_use_ssl: bool,
    smtp_user: String,
    smtp_password_env: String,
    timeout_seconds: u64,
}

fn parse_bool(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_args(raw_args: &[String]) -> Result<Args> {
    let mut out = Args {
        delivery_type: "webhook".to_string(),
        endpoint: String::new(),
        recipient: String::new(),
        payload_json: "{}".to_string(),
        alert_level: "info".to_string(),
        source: "cycle".to_string(),
        smtp_server: String::new(),
        smtp_port: 25,
        smtp_from: String::new(),
        smtp_use_ssl: false,
        smtp_user: String::new(),
        smtp_password_env: String::new(),
        timeout_seconds: 5,
    };
    let mut it = raw_args.iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--delivery-type" => out.delivery_type = val.clone(),
            "--endpoint" => out.endpoint = val.clone(),
            "--recipient" => out.recipient = val.clone(),
            "--payload-json" => out.payload_json = val.clone(),
            "--alert-level" => out.alert_level = val.clone(),
            "--source" => out.source = val.clone(),
            "--smtp-server" => out.smtp_server = val.clone(),
            "--smtp-port" => out.smtp_port = val.parse().unwrap_or(25),
            "--smtp-from" => out.smtp_from = val.clone(),
            "--smtp-use-ssl" => out.smtp_use_ssl = parse_bool(val),
            "--smtp-user" => out.smtp_user = val.clone(),
            "--smtp-password-env" => out.smtp_password_env = val.clone(),
            "--timeout-seconds" => out.timeout_seconds = val.parse().unwrap_or(5),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    Ok(out)
}

fn normalize_delivery_type(raw: &str) -> String {
    let v = raw.trim().to_ascii_lowercase();
    if v.is_empty() {
        "webhook".to_string()
    } else {
        v
    }
}

fn send_webhook(endpoint: &str, payload_json: &str, _timeout_seconds: u64) -> Result<()> {
    match ureq::post(endpoint)
        .set("Content-Type", "application/json")
        .send_string(payload_json)
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            bail!("http status {} {}", code, body.trim());
        }
        Err(e) => bail!("http request failed: {}", e),
    }
}

#[allow(clippy::too_many_arguments)]
fn send_email(
    recipient: &str,
    payload_json: &str,
    alert_level: &str,
    source: &str,
    smtp_server: &str,
    smtp_port: u16,
    smtp_from: &str,
    smtp_use_ssl: bool,
    smtp_user: &str,
    smtp_password_env: &str,
) -> Result<()> {
    let from: Mailbox = smtp_from
        .parse()
        .with_context(|| format!("invalid smtp from: {}", smtp_from))?;
    let to: Mailbox = recipient
        .parse()
        .with_context(|| format!("invalid recipient: {}", recipient))?;
    let subject = format!(
        "[NOVOVM][{}][{}] rollout_decision_summary",
        alert_level, source
    );
    let msg = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .body(payload_json.to_string())
        .context("build email failed")?;

    let mut builder = if smtp_use_ssl {
        SmtpTransport::relay(smtp_server)
            .with_context(|| format!("init smtp relay failed: {}", smtp_server))?
    } else {
        SmtpTransport::builder_dangerous(smtp_server)
    };
    builder = builder.port(smtp_port);

    if !smtp_user.trim().is_empty() && !smtp_password_env.trim().is_empty() {
        let pwd = env::var(smtp_password_env).unwrap_or_default();
        if !pwd.trim().is_empty() {
            builder = builder.credentials(Credentials::new(smtp_user.to_string(), pwd));
        }
    }

    let mailer = builder.build();
    mailer.send(&msg).context("smtp send failed")?;
    Ok(())
}

pub fn run_with_args(raw_args: &[String]) -> Result<()> {
    let args = parse_args(raw_args)?;
    let delivery_type = normalize_delivery_type(&args.delivery_type);
    let endpoint = args.endpoint.trim().to_string();
    let recipient = args.recipient.trim().to_string();

    let (status, ok, error) = if delivery_type == "webhook" || delivery_type == "im" {
        if endpoint.is_empty() {
            ("no_endpoint".to_string(), false, String::new())
        } else {
            match send_webhook(&endpoint, &args.payload_json, args.timeout_seconds) {
                Ok(_) => ("sent".to_string(), true, String::new()),
                Err(e) => ("failed".to_string(), false, e.to_string()),
            }
        }
    } else if delivery_type == "email" {
        if recipient.is_empty() {
            ("no_recipient".to_string(), false, String::new())
        } else if args.smtp_server.trim().is_empty() || args.smtp_from.trim().is_empty() {
            ("no_smtp_config".to_string(), false, String::new())
        } else {
            match send_email(
                &recipient,
                &args.payload_json,
                &args.alert_level,
                &args.source,
                &args.smtp_server,
                args.smtp_port,
                &args.smtp_from,
                args.smtp_use_ssl,
                &args.smtp_user,
                &args.smtp_password_env,
            ) {
                Ok(_) => ("sent".to_string(), true, String::new()),
                Err(e) => ("failed".to_string(), false, e.to_string()),
            }
        }
    } else {
        ("unsupported_type".to_string(), false, String::new())
    };

    let out = json!({
        "status": status,
        "ok": ok,
        "endpoint": endpoint,
        "recipient": recipient,
        "error": error
    });
    println!(
        "{}",
        serde_json::to_string(&out).context("serialize output failed")?
    );
    Ok(())
}

pub fn run_cli() -> Result<()> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    run_with_args(&raw_args)
}
