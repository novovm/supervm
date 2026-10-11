//! Shared launch selection for the lightweight and node relay executables.
//!
//! The historical positional form continues to select the legacy JSON runtime.
//! Duplex wire v2 is an explicit choice, not a connection-time negotiation or a
//! fallback. This module adds no protocol, identity, or consensus behavior.

use anyhow::{bail, Result};
use std::{
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
};

const USAGE: &str =
    "usage: <relay-executable> [--runtime legacy-json|duplex-v2] <relay-config.json>";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayDaemonRuntimeV1 {
    LegacyJson,
    DuplexV2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRelayLaunchV1 {
    pub runtime: RelayDaemonRuntimeV1,
    pub config_path: PathBuf,
    /// True only for the historical single positional argument.
    pub legacy_positional: bool,
}

/// Parse arguments after the executable name. Unknown, duplicate, incomplete,
/// and surplus arguments are rejected before either runtime reads a file.
/// Paths are kept as OS strings; a path starting with `-` needs a directory
/// prefix (for example `./-relay.json`) to distinguish it from an option.
pub fn parse_relay_launch_v1<I, S>(args: I) -> Result<ProductRelayLaunchV1>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    // Four items are sufficient to reject surplus input without collecting an
    // arbitrarily large iterator. The accepted forms contain one or three.
    let args: Vec<OsString> = args.into_iter().take(4).map(Into::into).collect();
    let (runtime, path, legacy_positional) = match args.as_slice() {
        [path] => (RelayDaemonRuntimeV1::LegacyJson, path, true),
        [option, runtime, path] if option == "--runtime" => {
            let runtime = if runtime == "legacy-json" {
                RelayDaemonRuntimeV1::LegacyJson
            } else if runtime == "duplex-v2" {
                RelayDaemonRuntimeV1::DuplexV2
            } else {
                bail!("unsupported relay runtime; {USAGE}");
            };
            (runtime, path, false)
        }
        _ => bail!("invalid relay arguments; {USAGE}"),
    };
    if path.is_empty() || path.to_string_lossy().starts_with('-') {
        bail!("invalid relay config path; {USAGE}");
    }
    Ok(ProductRelayLaunchV1 {
        runtime,
        config_path: PathBuf::from(path),
        legacy_positional,
    })
}

/// Both executable entrypoints use this same selector. `legacy` is supplied by
/// the executable to preserve its existing implementation without pulling node
/// or execution-engine dependencies into the network crate.
///
/// A selected backend's error is returned directly. In particular, duplex
/// configuration, TLS, bind, or protocol errors never invoke `legacy`.
pub fn run_product_relay_cli_v1<I, S, F>(args: I, legacy: F) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    F: FnOnce(&Path) -> Result<()>,
{
    let launch = parse_relay_launch_v1(args)?;
    {
        let mut stderr = std::io::stderr().lock();
        match launch.runtime {
            RelayDaemonRuntimeV1::LegacyJson if launch.legacy_positional => writeln!(
                stderr,
                "relay runtime: legacy-json (historical positional invocation; use --runtime duplex-v2 to select duplex)"
            )?,
            RelayDaemonRuntimeV1::LegacyJson => {
                writeln!(stderr, "relay runtime: legacy-json (explicit)")?;
            }
            RelayDaemonRuntimeV1::DuplexV2 => {
                writeln!(stderr, "relay runtime: duplex-v2 (explicit; no legacy fallback)")?;
            }
        }
    }
    dispatch_relay_launch_v1(launch, legacy, |path| {
        use crate::duplex::product_relay_daemon::{
            load_product_relay_daemon_config_v1, run_product_relay_daemon_v1,
        };
        let config = load_product_relay_daemon_config_v1(path)?;
        run_product_relay_daemon_v1(config)
    })
}

fn dispatch_relay_launch_v1<L, D>(launch: ProductRelayLaunchV1, legacy: L, duplex: D) -> Result<()>
where
    L: FnOnce(&Path) -> Result<()>,
    D: FnOnce(&Path) -> Result<()>,
{
    match launch.runtime {
        RelayDaemonRuntimeV1::LegacyJson => legacy(&launch.config_path),
        RelayDaemonRuntimeV1::DuplexV2 => duplex(&launch.config_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, fs::OpenOptions};

    #[test]
    fn historical_and_explicit_legacy_are_distinguishable() {
        let historical = parse_relay_launch_v1(["old config.json"]).unwrap();
        assert_eq!(historical.runtime, RelayDaemonRuntimeV1::LegacyJson);
        assert!(historical.legacy_positional);
        assert_eq!(historical.config_path, Path::new("old config.json"));
        let explicit =
            parse_relay_launch_v1(["--runtime", "legacy-json", "old config.json"]).unwrap();
        assert_eq!(explicit.runtime, RelayDaemonRuntimeV1::LegacyJson);
        assert!(!explicit.legacy_positional);
        assert_eq!(explicit.config_path, historical.config_path);
    }

    #[test]
    fn duplex_requires_its_exact_explicit_selector() {
        let launch = parse_relay_launch_v1(["--runtime", "duplex-v2", "new.json"]).unwrap();
        assert_eq!(launch.runtime, RelayDaemonRuntimeV1::DuplexV2);
        assert!(!launch.legacy_positional);
        assert_eq!(launch.config_path, Path::new("new.json"));
        // A positional filename is never inferred to be the duplex runtime.
        assert_eq!(
            parse_relay_launch_v1(["duplex-v2"]).unwrap().runtime,
            RelayDaemonRuntimeV1::LegacyJson
        );
    }

    #[test]
    fn unknown_missing_duplicate_and_surplus_arguments_fail_before_dispatch() {
        let cases: &[&[&str]] = &[
            &[],
            &[""],
            &["--help"],
            &["--runtime"],
            &["--runtime", "duplex-v2"],
            &["--runtime", "duplex", "config.json"],
            &["--runtime", "DUPLEX-V2", "config.json"],
            &["--mode", "duplex-v2", "config.json"],
            &["--runtime=duplex-v2", "config.json"],
            &["--runtime", "duplex-v2", ""],
            &["--runtime", "duplex-v2", "--runtime"],
            &["config.json", "extra"],
            &["config.json", "--runtime", "duplex-v2"],
            &["--runtime", "duplex-v2", "config.json", "extra"],
            &[
                "--runtime",
                "duplex-v2",
                "--runtime",
                "legacy-json",
                "config.json",
            ],
        ];
        for args in cases {
            let called = Cell::new(false);
            assert!(run_product_relay_cli_v1(args.iter().copied(), |_| {
                called.set(true);
                Ok(())
            })
            .is_err());
            assert!(!called.get(), "invalid arguments dispatched a runtime");
        }
    }

    #[test]
    fn explicit_dispatch_calls_only_the_selected_backend_once() {
        for (name, expected) in [
            ("legacy-json", RelayDaemonRuntimeV1::LegacyJson),
            ("duplex-v2", RelayDaemonRuntimeV1::DuplexV2),
        ] {
            let legacy_calls = Cell::new(0);
            let duplex_calls = Cell::new(0);
            let launch = parse_relay_launch_v1(["--runtime", name, "fixture.json"]).unwrap();
            dispatch_relay_launch_v1(
                launch,
                |path| {
                    assert_eq!(path, Path::new("fixture.json"));
                    legacy_calls.set(legacy_calls.get() + 1);
                    Ok(())
                },
                |path| {
                    assert_eq!(path, Path::new("fixture.json"));
                    duplex_calls.set(duplex_calls.get() + 1);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(
                (legacy_calls.get(), duplex_calls.get()),
                if expected == RelayDaemonRuntimeV1::LegacyJson {
                    (1, 0)
                } else {
                    (0, 1)
                }
            );
        }
    }

    #[test]
    fn selected_duplex_failure_is_returned_without_legacy_fallback() {
        let legacy_calls = Cell::new(0);
        let launch = parse_relay_launch_v1(["--runtime", "duplex-v2", "fixture.json"]).unwrap();
        let result = dispatch_relay_launch_v1(
            launch,
            |_| {
                legacy_calls.set(legacy_calls.get() + 1);
                Ok(())
            },
            |_| Err(anyhow::anyhow!("selected duplex fixture failure")),
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "selected duplex fixture failure"
        );
        assert_eq!(legacy_calls.get(), 0);
    }

    #[test]
    fn real_duplex_config_decode_failure_never_calls_legacy() {
        struct ConfigFile(PathBuf);
        impl Drop for ConfigFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let path = std::env::temp_dir().join(format!(
            "novovm-relay-launch-fixture-{}-{:032x}.json",
            std::process::id(),
            rand::random::<u128>()
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        let owned = ConfigFile(path);
        file.write_all(b"{ not a relay config }").unwrap();
        drop(file);
        let legacy_calls = Cell::new(0);
        let result = run_product_relay_cli_v1(
            [
                OsString::from("--runtime"),
                OsString::from("duplex-v2"),
                owned.0.as_os_str().to_owned(),
            ],
            |_| {
                legacy_calls.set(legacy_calls.get() + 1);
                Ok(())
            },
        );
        let error = result.unwrap_err();
        assert!(error.downcast_ref::<serde_json::Error>().is_some());
        assert_eq!(legacy_calls.get(), 0);
    }
}
