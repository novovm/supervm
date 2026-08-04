use std::path::{Path, PathBuf};

use crate::error::CtlError;

const SUPERVM_ROOT_ENV_V1: &str = "SUPERVM_ROOT";

pub fn resolve_policy_binary(explicit: Option<&str>) -> Result<String, CtlError> {
    if let Some(path) = explicit {
        validate_exists(path, "novovm-rollout-policy")?;
        return Ok(path.to_string());
    }

    let candidates = default_policy_candidates();
    first_existing(&candidates).ok_or_else(|| {
        CtlError::BinaryNotFound(format!(
            "novovm-rollout-policy not found; tried: {}",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

pub fn resolve_node_binary(explicit: Option<&str>) -> Result<String, CtlError> {
    if let Some(path) = explicit {
        validate_exists(path, "novovm-node")?;
        return Ok(path.to_string());
    }

    let candidates = default_node_candidates();
    first_existing(&candidates).ok_or_else(|| {
        CtlError::BinaryNotFound(format!(
            "novovm-node not found; tried: {}",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

fn default_policy_candidates() -> Vec<PathBuf> {
    let repo_root = discover_supervm_root_v1();
    let target_dir = configured_cargo_target_dir(repo_root.as_deref());
    policy_candidates_v1(repo_root.as_deref(), target_dir.as_deref())
}

fn policy_candidates_v1(repo_root: Option<&Path>, target_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = external_target_candidates("novovm-rollout-policy", target_dir);
    candidates.extend(
        [
            "target/release/novovm-rollout-policy.exe",
            "target/release/novovm-rollout-policy",
            "target/debug/novovm-rollout-policy.exe",
            "target/debug/novovm-rollout-policy",
            "crates/novovm-rollout-policy/target/release/novovm-rollout-policy.exe",
            "crates/novovm-rollout-policy/target/release/novovm-rollout-policy",
            "crates/novovm-rollout-policy/target/debug/novovm-rollout-policy.exe",
            "crates/novovm-rollout-policy/target/debug/novovm-rollout-policy",
        ]
        .into_iter()
        .map(|relative| repo_anchored_candidate_v1(repo_root, relative)),
    );
    candidates
}

fn default_node_candidates() -> Vec<PathBuf> {
    let repo_root = discover_supervm_root_v1();
    let target_dir = configured_cargo_target_dir(repo_root.as_deref());
    node_candidates_v1(repo_root.as_deref(), target_dir.as_deref())
}

fn node_candidates_v1(repo_root: Option<&Path>, target_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = external_target_candidates("novovm-node", target_dir);
    candidates.extend(
        [
            "target/release/novovm-node.exe",
            "target/release/novovm-node",
            "target/debug/novovm-node.exe",
            "target/debug/novovm-node",
            "crates/novovm-node/target/release/novovm-node.exe",
            "crates/novovm-node/target/release/novovm-node",
            "crates/novovm-node/target/debug/novovm-node.exe",
            "crates/novovm-node/target/debug/novovm-node",
        ]
        .into_iter()
        .map(|relative| repo_anchored_candidate_v1(repo_root, relative)),
    );
    candidates
}

fn configured_cargo_target_dir(repo_root: Option<&Path>) -> Option<PathBuf> {
    std::env::var_os("CARGO_TARGET_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|path| anchor_runtime_path_v1(path.as_path(), repo_root))
}

fn discover_supervm_root_v1() -> Option<PathBuf> {
    let explicit_root = std::env::var_os(SUPERVM_ROOT_ENV_V1)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let executable = std::env::current_exe().ok();
    let working_dir = std::env::current_dir().ok();
    discover_supervm_root_from_paths_v1(
        explicit_root.as_deref(),
        executable.as_deref(),
        working_dir.as_deref(),
    )
}

fn discover_supervm_root_from_paths_v1(
    explicit_root: Option<&Path>,
    executable: Option<&Path>,
    working_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(root) = explicit_root
        .and_then(absolute_path_v1)
        .filter(|root| is_supervm_root_v1(root.as_path()))
    {
        return Some(root);
    }
    for start in [executable, working_dir].into_iter().flatten() {
        for ancestor in start.ancestors() {
            if is_supervm_root_v1(ancestor) {
                return absolute_path_v1(ancestor);
            }
        }
    }
    None
}

fn is_supervm_root_v1(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("crates/novovm-node/Cargo.toml").is_file()
        && path
            .join("crates/novovm-rollout-policy/Cargo.toml")
            .is_file()
}

fn absolute_path_v1(path: &Path) -> Option<PathBuf> {
    std::path::absolute(path).ok()
}

fn anchor_runtime_path_v1(path: &Path, repo_root: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    repo_root
        .map(|root| root.join(path))
        .or_else(|| absolute_path_v1(path))
        .unwrap_or_else(|| path.to_path_buf())
}

fn repo_anchored_candidate_v1(repo_root: Option<&Path>, relative: &str) -> PathBuf {
    repo_root
        .map(|root| root.join(relative))
        .unwrap_or_else(|| PathBuf::from(relative))
}

fn external_target_candidates(binary_name: &str, target_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(target_dir) = target_dir {
        candidates.extend([
            target_dir
                .join("release")
                .join(format!("{binary_name}.exe")),
            target_dir.join("release").join(binary_name),
            target_dir.join("debug").join(format!("{binary_name}.exe")),
            target_dir.join("debug").join(binary_name),
        ]);
    }
    candidates
}

fn first_existing(candidates: &[PathBuf]) -> Option<String> {
    candidates
        .iter()
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

fn validate_exists(path: &str, name: &str) -> Result<(), CtlError> {
    if Path::new(path).is_file() {
        Ok(())
    } else {
        Err(CtlError::BinaryNotFound(format!(
            "{name} explicit path not found: {path}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_SERIAL_V1: AtomicU64 = AtomicU64::new(1);

    fn fake_repo_v1(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "novovmctl-paths-{label}-{}-{}",
            std::process::id(),
            TEST_SERIAL_V1.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("crates/novovm-node")).expect("create node marker dir");
        fs::create_dir_all(root.join("crates/novovm-rollout-policy"))
            .expect("create policy marker dir");
        fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("write workspace marker");
        fs::write(root.join("crates/novovm-node/Cargo.toml"), "[package]\n")
            .expect("write node marker");
        fs::write(
            root.join("crates/novovm-rollout-policy/Cargo.toml"),
            "[package]\n",
        )
        .expect("write policy marker");
        root
    }

    #[test]
    fn executable_ancestor_anchors_candidates_when_called_from_outside_repo() {
        let root = fake_repo_v1("external-cwd");
        let executable = root.join("target/debug/novovmctl.exe");
        let external_cwd = std::env::temp_dir().join(format!(
            "novovmctl-external-cwd-{}-{}",
            std::process::id(),
            TEST_SERIAL_V1.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(external_cwd.as_path()).expect("create external cwd");
        let discovered = discover_supervm_root_from_paths_v1(
            None,
            Some(executable.as_path()),
            Some(external_cwd.as_path()),
        )
        .expect("discover repo from executable");
        assert_eq!(
            discovered,
            absolute_path_v1(root.as_path()).expect("absolute repo")
        );

        let mut candidates = policy_candidates_v1(Some(discovered.as_path()), None);
        candidates.extend(node_candidates_v1(Some(discovered.as_path()), None));
        assert!(!candidates.is_empty());
        for candidate in candidates {
            assert!(
                candidate.is_absolute() && candidate.starts_with(discovered.as_path()),
                "candidate is not anchored to the discovered repo: {}",
                candidate.display()
            );
        }
        fs::remove_dir_all(root).expect("remove fake repo");
        fs::remove_dir_all(external_cwd).expect("remove external cwd");
    }

    #[test]
    fn relative_target_dir_is_repo_anchored_and_candidates_use_only_that_target() {
        let root = fake_repo_v1("target-dir");
        let target_dir =
            anchor_runtime_path_v1(Path::new("portable-cargo-target"), Some(root.as_path()));
        assert_eq!(target_dir, root.join("portable-cargo-target"));
        let candidates = external_target_candidates("novovm-node", Some(target_dir.as_path()));
        assert_eq!(candidates.len(), 4);
        assert!(candidates
            .iter()
            .all(|candidate| candidate.starts_with(target_dir.as_path())));
        assert_eq!(
            candidates[0],
            target_dir.join("release").join("novovm-node.exe")
        );
        assert_eq!(candidates[3], target_dir.join("debug").join("novovm-node"));
        fs::remove_dir_all(root).expect("remove fake repo");
    }

    #[test]
    fn directories_never_shadow_real_binary_candidates() {
        let root = fake_repo_v1("is-file");
        let directory = root.join("directory-candidate");
        let binary = root.join("binary-candidate");
        fs::create_dir_all(directory.as_path()).expect("create directory candidate");
        fs::write(binary.as_path(), b"binary").expect("create binary candidate");
        assert_eq!(
            first_existing(&[directory, binary.clone()]),
            Some(binary.display().to_string())
        );
        assert!(validate_exists(root.as_path().to_string_lossy().as_ref(), "directory").is_err());
        fs::remove_dir_all(root).expect("remove fake repo");
    }
}
