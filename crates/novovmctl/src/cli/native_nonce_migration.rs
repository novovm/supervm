use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(about = "Offline legacy nonce checkpoint review; never imports or activates state")]
pub struct NativeNonceMigrationArgs {
    #[command(subcommand)]
    pub command: NativeNonceMigrationCommand,
}

#[derive(Debug, Subcommand)]
pub enum NativeNonceMigrationCommand {
    /// Observe an offline snapshot and ledger; observed values are not trusted pins.
    Inspect(NativeNonceInspectArgs),
    /// Validate explicit checkpoint pins and create a new portable evidence bundle.
    Export(NativeNonceExportArgs),
    /// Verify a transported bundle against independently supplied digest and pins.
    Verify(NativeNonceVerifyArgs),
    /// Observe this binary's current V2 protocol commitment; not upgrade authority.
    TargetProtocol,
    /// Stage a verified V2 state proposal in a new, separate offline workspace.
    PrepareUpgrade(NativeNonceUpgradeArgs),
    /// Resume an existing offline proposal journal without publishing chain state.
    ResumeUpgrade(NativeNonceUpgradeArgs),
    /// Revalidate an existing proposal journal without writing any artifacts.
    InspectUpgrade(NativeNonceUpgradeArgs),
    /// Verify a separately supplied upgrade certificate without signing or publishing state.
    VerifyUpgradeAuthorization(NativeNonceUpgradeAuthorizationArgs),
    /// Verify source prepare-QC coverage without claiming finality or replaying execution.
    VerifySourceQc(NativeNonceSourceQcArgs),
}

#[derive(Debug, Args)]
pub struct NativeNonceInspectArgs {
    #[arg(long, value_name = "FILE")]
    pub snapshot: PathBuf,
    #[arg(long, value_name = "DIRECTORY")]
    pub ledger: PathBuf,
    #[arg(long)]
    pub chain_id: u64,
}

#[derive(Debug, Args)]
pub struct NativeNonceCheckpointArgs {
    #[arg(long)]
    pub chain_id: u64,
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub namespace_digest: String,
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub legacy_protocol_commitment: String,
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub tip_block_hash: String,
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub snapshot_digest: String,
}

#[derive(Debug, Args)]
pub struct NativeNonceExportArgs {
    #[arg(long, value_name = "FILE")]
    pub snapshot: PathBuf,
    #[arg(long, value_name = "DIRECTORY")]
    pub ledger: PathBuf,
    #[command(flatten)]
    pub checkpoint: NativeNonceCheckpointArgs,
    /// New file only; parent must exist and must be outside the ledger directory.
    #[arg(long, value_name = "NEW_FILE")]
    pub bundle_out: PathBuf,
}

#[derive(Debug, Args)]
pub struct NativeNonceVerifyArgs {
    #[arg(long, value_name = "FILE")]
    pub bundle: PathBuf,
    #[command(flatten)]
    pub checkpoint: NativeNonceCheckpointArgs,
    /// Expected digest supplied independently of the transported bundle.
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub bundle_digest: String,
}

#[derive(Debug, Args)]
pub struct NativeNonceUpgradeArgs {
    #[command(flatten)]
    pub evidence: NativeNonceVerifyArgs,
    /// Explicit V2 commitment; must match this binary's current environment.
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub target_protocol_commitment: String,
    /// Separate proposal directory; prepare requires a new directory.
    #[arg(long, value_name = "DIRECTORY")]
    pub workspace: PathBuf,
}

#[derive(Debug, Args)]
pub struct NativeNonceUpgradeAuthorizationArgs {
    #[command(flatten)]
    pub evidence: NativeNonceVerifyArgs,
    /// Explicit V2 commitment; must match this binary's current environment.
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub target_protocol_commitment: String,
    /// Existing epoch authority document, not a request to create authority.
    #[arg(long, value_name = "FILE")]
    pub authority: PathBuf,
    /// Authority commitment obtained independently of the supplied documents.
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub expected_authority_commitment: String,
    #[arg(long, value_name = "FILE")]
    pub certificate: PathBuf,
}

#[derive(Debug, Args)]
pub struct NativeNonceSourceQcArgs {
    #[command(flatten)]
    pub evidence: NativeNonceVerifyArgs,
    /// Existing old-chain authority document, not newly granted authority.
    #[arg(long, value_name = "FILE")]
    pub authority: PathBuf,
    /// Old-chain authority commitment obtained independently of these documents.
    #[arg(long, value_name = "LOWERCASE_HEX_32")]
    pub expected_authority_commitment: String,
    #[arg(long, value_name = "FILE")]
    pub source_qc: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, TopCommand};
    use clap::Parser;

    fn checkpoint_flags() -> Vec<String> {
        vec![
            "--chain-id".into(),
            "7".into(),
            "--namespace-digest".into(),
            "ab".repeat(32),
            "--legacy-protocol-commitment".into(),
            "cd".repeat(32),
            "--tip-block-hash".into(),
            "ef".repeat(32),
            "--snapshot-digest".into(),
            "12".repeat(32),
        ]
    }

    #[test]
    fn native_nonce_migration_parser_requires_explicit_sources_and_pins() {
        assert!(Cli::try_parse_from(["novovmctl", "native-nonce-migration", "inspect"]).is_err());
        let cli = Cli::try_parse_from([
            "novovmctl",
            "native-nonce-migration",
            "inspect",
            "--snapshot",
            "old.json",
            "--ledger",
            "ledger.rocksdb",
            "--chain-id",
            "7",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            TopCommand::NativeNonceMigration(NativeNonceMigrationArgs {
                command: NativeNonceMigrationCommand::Inspect(_)
            })
        ));
        let mut export = vec![
            "novovmctl".into(),
            "native-nonce-migration".into(),
            "export".into(),
            "--snapshot".into(),
            "old.json".into(),
            "--ledger".into(),
            "ledger.rocksdb".into(),
            "--bundle-out".into(),
            "bundle.json".into(),
        ];
        assert!(Cli::try_parse_from(&export).is_err());
        export.extend(checkpoint_flags());
        assert!(Cli::try_parse_from(&export).is_ok());
        export.push("--force".into());
        assert!(Cli::try_parse_from(&export).is_err());
        for command in ["import", "activate"] {
            assert!(Cli::try_parse_from(["novovmctl", "native-nonce-migration", command]).is_err());
        }
    }

    #[test]
    fn native_nonce_migration_verify_requires_independent_bundle_digest() {
        let mut args = vec![
            "novovmctl".into(),
            "native-nonce-migration".into(),
            "verify".into(),
            "--bundle".into(),
            "bundle.json".into(),
        ];
        args.extend(checkpoint_flags());
        assert!(Cli::try_parse_from(&args).is_err());
        args.extend(["--bundle-digest".into(), "34".repeat(32)]);
        assert!(Cli::try_parse_from(args).is_ok());
    }

    #[test]
    fn native_nonce_upgrade_parser_requires_explicit_target_and_workspace() {
        assert!(
            Cli::try_parse_from(["novovmctl", "native-nonce-migration", "target-protocol",])
                .is_ok()
        );
        for action in ["prepare-upgrade", "resume-upgrade", "inspect-upgrade"] {
            let mut args = vec![
                "novovmctl".into(),
                "native-nonce-migration".into(),
                action.into(),
                "--bundle".into(),
                "checkpoint.bin".into(),
                "--bundle-digest".into(),
                "34".repeat(32),
            ];
            args.extend(checkpoint_flags());
            assert!(Cli::try_parse_from(&args).is_err());
            args.extend(["--target-protocol-commitment".into(), "56".repeat(32)]);
            assert!(Cli::try_parse_from(&args).is_err());
            args.extend(["--workspace".into(), "offline-proposal".into()]);
            assert!(Cli::try_parse_from(&args).is_ok());
            args.push("--force".into());
            assert!(Cli::try_parse_from(&args).is_err());
        }
    }

    #[test]
    fn native_nonce_upgrade_authorization_parser_requires_all_pins_and_documents() {
        let mut args = vec![
            "novovmctl".into(),
            "native-nonce-migration".into(),
            "verify-upgrade-authorization".into(),
            "--bundle".into(),
            "checkpoint.bin".into(),
            "--bundle-digest".into(),
            "34".repeat(32),
            "--target-protocol-commitment".into(),
            "56".repeat(32),
            "--authority".into(),
            "authority.json".into(),
            "--expected-authority-commitment".into(),
            "78".repeat(32),
            "--certificate".into(),
            "certificate.json".into(),
        ];
        args.extend(checkpoint_flags());
        assert!(Cli::try_parse_from(&args).is_ok());
        for offset in (3..args.len()).step_by(2) {
            let mut missing = args.clone();
            missing.drain(offset..offset + 2);
            assert!(
                Cli::try_parse_from(&missing).is_err(),
                "required {}",
                args[offset]
            );
        }
        for forbidden in ["--private-key", "--sign", "--activate", "--force"] {
            let mut unsupported = args.clone();
            unsupported.push(forbidden.into());
            assert!(Cli::try_parse_from(&unsupported).is_err());
        }
    }

    #[test]
    fn native_nonce_source_qc_parser_requires_old_chain_pins_without_upgrade_authority() {
        let mut args = vec![
            "novovmctl".into(),
            "native-nonce-migration".into(),
            "verify-source-qc".into(),
            "--bundle".into(),
            "checkpoint.bin".into(),
            "--bundle-digest".into(),
            "34".repeat(32),
            "--authority".into(),
            "authority.json".into(),
            "--expected-authority-commitment".into(),
            "56".repeat(32),
            "--source-qc".into(),
            "source-qc.json".into(),
        ];
        args.extend(checkpoint_flags());
        let cli = Cli::try_parse_from(&args).unwrap();
        assert!(matches!(
            cli.command,
            TopCommand::NativeNonceMigration(NativeNonceMigrationArgs {
                command: NativeNonceMigrationCommand::VerifySourceQc(_)
            })
        ));
        for offset in (3..args.len()).step_by(2) {
            let mut missing = args.clone();
            missing.drain(offset..offset + 2);
            assert!(
                Cli::try_parse_from(&missing).is_err(),
                "required {}",
                args[offset]
            );
        }
        for forbidden in [
            "--target-protocol-commitment",
            "--certificate",
            "--workspace",
            "--private-key",
            "--sign",
            "--activate",
            "--force",
        ] {
            let mut unsupported = args.clone();
            unsupported.extend([forbidden.into(), "forbidden".into()]);
            assert!(Cli::try_parse_from(&unsupported).is_err());
        }
        for command in ["export-source-qc", "sign-source-qc", "import-source-qc"] {
            assert!(Cli::try_parse_from(["novovmctl", "native-nonce-migration", command]).is_err());
        }
    }
}
