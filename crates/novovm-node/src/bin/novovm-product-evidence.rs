use anyhow::{bail, Context, Result};
use novovm_node::product_evidence::{
    build_product_evidence_manifest_v1, now_ms_v1, verify_product_evidence_manifest_v1,
    write_product_evidence_manifest_v1,
};
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("build") => {
            let root = PathBuf::from(args.next().context("usage: novovm-product-evidence build <root> <chain-id> <protected-transport-key> <library> <sha256> <manifest.json> <report>...")?);
            let chain_id = args.next().context("missing chain id")?.parse()?;
            let key = PathBuf::from(args.next().context("missing protected evidence identity")?);
            let library = PathBuf::from(args.next().context("missing AOEM library")?);
            let sha = args.next().context("missing approved AOEM SHA256")?;
            let identity = novovm_relay::transport_identity::load_transport_identity(
                &key, chain_id, &library, &sha,
            )?;
            let manifest_path =
                PathBuf::from(args.next().context("missing evidence manifest path")?);
            let reports = args.map(PathBuf::from).collect::<Vec<_>>();
            let manifest =
                build_product_evidence_manifest_v1(&root, &reports, &identity, now_ms_v1())?;
            write_product_evidence_manifest_v1(&manifest_path, &manifest)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        Some("verify") => {
            let root = PathBuf::from(args.next().context(
                "usage: novovm-product-evidence verify <root> <library> <sha256> <manifest.json>",
            )?);
            let library = PathBuf::from(args.next().context("missing AOEM library")?);
            let sha = args.next().context("missing approved AOEM SHA256")?;
            let verifier =
                novovm_relay::transport_identity::load_transport_verifier(&library, &sha)?;
            let manifest = PathBuf::from(args.next().context("missing evidence manifest path")?);
            if args.next().is_some() {
                bail!("too many arguments for evidence verify");
            }
            let result = verify_product_evidence_manifest_v1(&root, &manifest, &verifier);
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !result.accepted {
                bail!("product evidence verification failed");
            }
        }
        _ => bail!("usage: novovm-product-evidence <build|verify> ..."),
    }
    Ok(())
}
