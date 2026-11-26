use super::*;
use crate::native_pipeline::state::tree::state_node_children;

#[test]
#[ignore = "requires a copied height-42 follow-state cache and pinned test profile"]
fn authentic_height_42_cache_answers_only_certified_read_queries() -> Result<()> {
    let data =
        PathBuf::from(std::env::var_os("NOVOVM_READ_SNAPSHOT_TEST_DATA").context("test cache")?);
    let profile = PathBuf::from(
        std::env::var_os("NOVOVM_READ_SNAPSHOT_TEST_PROFILE").context("test profile")?,
    );
    let pin = std::env::var("NOVOVM_READ_SNAPSHOT_TEST_PROFILE_SHA256")?;
    let budget = NonZeroUsize::new(19).unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(120);
    let mut source = ReadSnapshot::open(&data, &profile, &pin, 42, budget, deadline)?;
    let Answer::Status(status) = source.answer(&Query::Status) else {
        panic!("snapshot status unavailable");
    };
    assert_eq!(status.height, 42);
    assert_eq!(
        hex(&status.block_hash),
        "0xf8db07a5ae5d2b3a2e716e89e49b8b827adabafbe3540639c2faa861157b0eee"
    );
    assert_eq!(
        hex(&status.state_root),
        "0x2a974401e58a564f84003811d9a546284c96e4cc8482a8baea7464460befe4ec"
    );
    let first = ArchiveSelector {
        height: 42,
        offset: 0,
        digest: [0; 32],
    };
    let Answer::FinalizedHeader(initial) = source.answer(&Query::FinalizedHeader(first)) else {
        panic!("snapshot header unavailable");
    };
    let mut assembled = initial.data;
    while assembled.len() < initial.total_bytes as usize {
        let next = ArchiveSelector {
            offset: assembled.len() as u32,
            digest: initial.digest,
            ..first
        };
        let Answer::FinalizedHeader(chunk) = source.answer(&Query::FinalizedHeader(next)) else {
            panic!("snapshot header continuation unavailable");
        };
        assert_eq!(
            (chunk.total_bytes, chunk.digest),
            (initial.total_bytes, initial.digest)
        );
        assembled.extend(chunk.data);
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(&assembled)),
        "d71b6da0e44198b0338f54de3d0836f88fa55c0f2b0a20bd98af48a1f8657ab5"
    );
    // Only the disposable copy is mutated. The current, fully verified
    // proof survives between chunks; an evicted proof must pass its digest again.
    let files = tempfile::tempdir()?;
    for height in [41, 42] {
        fs::copy(
            header_path(&source.headers, height),
            header_path(files.path(), height),
        )?;
    }
    source.headers = files.path().to_owned();
    source.cached_header = None;
    assert!(matches!(
        source.answer(&Query::FinalizedHeader(first)),
        Answer::FinalizedHeader(_)
    ));
    let proof = header_path(files.path(), 42);
    fs::write(&proof, [0u8; 8])?;
    let second = ArchiveSelector {
        offset: ARCHIVE_CHUNK_BYTES as u32,
        digest: initial.digest,
        ..first
    };
    if initial.total_bytes as usize > ARCHIVE_CHUNK_BYTES {
        assert!(matches!(
            source.answer(&Query::FinalizedHeader(second)),
            Answer::FinalizedHeader(_)
        ));
    }
    assert!(matches!(
        source.answer(&Query::FinalizedHeader(ArchiveSelector {
            height: 41,
            ..first
        })),
        Answer::FinalizedHeader(_)
    ));
    let deny = Answer::Error(QueryError::Unavailable);
    assert_eq!(source.answer(&Query::FinalizedHeader(first)), deny);
    assert!(ReadSnapshot::open(&data, &profile, &pin, 42, budget, Instant::now()).is_err());
    for invalid in [
        ArchiveSelector {
            height: 43,
            ..first
        },
        ArchiveSelector { offset: 1, ..first },
        ArchiveSelector {
            digest: initial.digest,
            ..first
        },
        ArchiveSelector {
            offset: ARCHIVE_CHUNK_BYTES as u32,
            digest: [7; 32],
            ..first
        },
    ] {
        assert_eq!(source.answer(&Query::FinalizedHeader(invalid)), deny);
    }
    assert_eq!(source.answer(&Query::ReceiptProof(first)), deny);
    let root = StateNodeSelector {
        height: 42,
        root: status.state_root,
        hash: status.state_root,
        ancestry: vec![],
    };
    let Answer::StateNode(StateNodePayload(branch)) =
        source.answer(&Query::StateNode(root.clone()))
    else {
        panic!("snapshot root unavailable");
    };
    let [left, _] = state_node_children(&root.hash, &branch)?.context("root must branch")?;
    let child = StateNodeSelector {
        hash: left,
        ancestry: branch,
        ..root.clone()
    };
    assert!(matches!(
        source.answer(&Query::StateNode(child.clone())),
        Answer::StateNode(_)
    ));
    for invalid in [
        StateNodeSelector {
            height: 41,
            ..child.clone()
        },
        StateNodeSelector {
            root: [4; 32],
            ..child.clone()
        },
        StateNodeSelector {
            hash: [5; 32],
            ..child.clone()
        },
        StateNodeSelector {
            ancestry: vec![],
            ..child
        },
    ] {
        assert_eq!(source.answer(&Query::StateNode(invalid)), deny);
    }
    assert!(ReadSnapshot::open(
        &data,
        &profile,
        &pin,
        42,
        NonZeroUsize::new(18).unwrap(),
        deadline
    )
    .is_err());
    assert!(ReadSnapshot::open(&data, &profile, &pin, 43, budget, deadline).is_err());
    println!("real height-42 snapshot: 42 verified headers, complete 19-node tree, query scope and key-free startup passed");
    Ok(())
}

#[test]
fn bounded_cache_file_rejects_special_or_oversized_input() -> Result<()> {
    let directory = std::env::temp_dir().join(format!(
        "novovm-read-snapshot-{:016x}",
        rand::random::<u64>()
    ));
    fs::create_dir(&directory)?;
    let result = (|| {
        assert!(read_cache_file(&directory, 3).is_err());
        let path = directory.join("proof");
        fs::write(&path, [1, 2, 3, 4])?;
        assert!(read_cache_file(&path, 3).is_err());
        assert_eq!(read_cache_file(&path, 4)?, vec![1, 2, 3, 4]);
        #[cfg(unix)]
        {
            let link = directory.join("link");
            std::os::unix::fs::symlink(&path, &link)?;
            assert!(read_cache_file(&link, 4).is_err());
            fs::remove_file(link)?;
        }
        fs::remove_file(path)?;
        Ok(())
    })();
    fs::remove_dir(directory)?;
    result
}
