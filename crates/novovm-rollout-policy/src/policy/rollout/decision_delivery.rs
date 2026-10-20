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
        .config()
        .proxy(None)
        .max_redirects(5)
        .timeout_connect(Some(std::time::Duration::from_secs(30)))
        // Preserve the status and error body for the existing refusal report.
        .http_status_as_error(false)
        .build()
        .header("Content-Type", "application/json")
        .send(payload_json)
    {
        Ok(mut resp) => {
            let code = resp.status().as_u16();
            if code >= 400 {
                let body = resp
                    .body_mut()
                    .read_to_vec()
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_default();
                bail!("http status {} {}", code, body.trim());
            }
            Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    #[test]
    fn webhook_http_status_preserves_refusal_body() {
        for (status, body, expected) in [
            (200, b"ok".as_slice(), None),
            (
                400,
                b" rejected ".as_slice(),
                Some("http status 400 rejected"),
            ),
            (
                503,
                b"bad\xff".as_slice(),
                Some("http status 503 bad\u{fffd}"),
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let endpoint = format!("http://{}/notify", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "webhook client did not connect");
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("webhook test accept failed: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let mut payload = [0; 2];
                stream.read_exact(&mut payload).unwrap();
                assert_eq!(&payload, b"{}");
                let headers = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(headers.starts_with("post /notify http/1.1\r\n"));
                assert!(headers.contains("content-type: application/json\r\n"));
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            });
            let result = send_webhook(&endpoint, "{}", 5);
            server.join().unwrap();
            match expected {
                Some(message) => assert_eq!(result.unwrap_err().to_string(), message),
                None => result.unwrap(),
            }
        }
    }
}
