use super::*;
use crate::native_pipeline::state::tree::{
    empty_root, state_node_children, visit_state_leaves, NodeHash, StateNodeReader,
};
use novovm_protocol::native_query::{
    QueryError, StateNodePayload, StateNodeSelector, MAX_STATE_NODE_BYTES, STATE_BRANCH_BYTES,
};

const MAX_NEW_NODES_PER_ATTEMPT: usize = 4096;

pub(super) struct StateCache(PathBuf);

pub(super) struct StateProgress {
    pub nodes: usize,
    pub leaves: usize,
    pub downloaded: usize,
}

impl StateCache {
    pub(super) fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self(root))
    }

    fn path(&self, hash: &NodeHash) -> PathBuf {
        use super::super::super::super::super::rpc::hex;
        self.0.join(format!("{}.node", hex(hash)))
    }

    pub(super) async fn sync(
        &self,
        session: &mut QuerySession<'_>,
        tip: ParentPoint,
    ) -> Result<StateProgress> {
        let mut downloaded = 0;
        if tip.state_root != empty_root() {
            let mut pending = vec![(tip.state_root, 0)];
            let mut ancestry = Vec::new();
            while let Some((hash, depth)) = pending.pop() {
                ancestry.truncate(depth * STATE_BRANCH_BYTES);
                let existing = self.read_node(&hash)?;
                let missing = existing.is_none();
                let bytes = if let Some(bytes) = existing {
                    bytes
                } else {
                    ensure!(
                        downloaded < MAX_NEW_NODES_PER_ATTEMPT,
                        "state download step budget reached"
                    );
                    let selector = StateNodeSelector {
                        height: tip.height,
                        root: tip.state_root,
                        hash,
                        ancestry: ancestry.clone(),
                    };
                    let StateNodePayload(bytes) = fetch_node(session, selector).await?;
                    bytes
                };
                let children = state_node_children(&hash, &bytes).with_context(|| {
                    format!("verifying state node {}", self.path(&hash).display())
                })?;
                if missing {
                    store_checkpoint(&self.path(&hash), &bytes)?;
                    downloaded += 1;
                }
                if let Some([left, right]) = children {
                    ensure!(depth < 256, "state snapshot exceeds Patricia depth");
                    ancestry.extend(bytes);
                    pending.push((right, depth + 1));
                    pending.push((left, depth + 1));
                }
            }
        }

        let mut leaves = 0;
        let nodes = visit_state_leaves(self, tip.state_root, usize::MAX, &mut |_, _| {
            leaves += 1;
            Ok(())
        })?;
        Ok(StateProgress {
            nodes,
            leaves,
            downloaded,
        })
    }
}

impl StateNodeReader for StateCache {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        let file = match fs::File::open(self.path(hash)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take((MAX_STATE_NODE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    }
}

async fn fetch_node(
    session: &mut QuerySession<'_>,
    selector: StateNodeSelector,
) -> Result<StateNodePayload> {
    loop {
        match session.query(Query::StateNode(selector.clone())).await? {
            Answer::StateNode(payload) => return Ok(payload),
            Answer::Error(QueryError::Busy) => tokio::time::sleep(Duration::from_millis(10)).await,
            _ => anyhow::bail!("authenticated peer cannot serve the certified state snapshot"),
        }
    }
}
