use super::*;

/// An invocation-local index produced only after every full ancestor was checked.
/// Reopening an indexed document authenticates its complete value again.
#[derive(Clone)]
pub(in crate::checkpoint) struct CheckedHistory {
    pub entries: Vec<Boundary>,
    session_id: String,
    session_sha256: String,
    head: Option<Head>,
}

#[derive(Clone)]
pub(in crate::checkpoint) struct Boundary {
    pub checkpoint_id: String,
    pub parent_id: Option<String>,
    pub sequence: u64,
    pub kind: String,
    pub component: Option<ComponentIdentity>,
    pub completed_components: usize,
    pub file_count: u64,
    pub total_bytes: u64,
    sha256: String,
}

impl Boundary {
    pub(super) fn capture(checkpoint: &Checkpoint) -> Result<Self> {
        Ok(Self {
            checkpoint_id: checkpoint.checkpoint_id.clone(),
            parent_id: checkpoint.parent_id.clone(),
            sequence: checkpoint.sequence,
            kind: checkpoint.kind.clone(),
            component: checkpoint.component.clone(),
            completed_components: checkpoint.completed_components.len(),
            file_count: checkpoint.file_count,
            total_bytes: checkpoint.total_bytes,
            sha256: format!("{:x}", Sha256::digest(canonical_bytes(checkpoint)?)),
        })
    }
}

impl CheckedHistory {
    pub(super) fn new(
        session: &Session,
        head: Option<Head>,
        entries: Vec<Boundary>,
    ) -> Result<Self> {
        Ok(Self {
            entries,
            session_id: session.session_id.clone(),
            session_sha256: format!("{:x}", Sha256::digest(canonical_bytes(session)?)),
            head,
        })
    }

    /// Only the existing pending-transition repair path may index an unpublished boundary.
    pub(in crate::checkpoint) fn pending(
        session: &Session,
        checkpoint: &Checkpoint,
    ) -> Result<Self> {
        Self::new(session, None, vec![Boundary::capture(checkpoint)?])
    }

    pub(in crate::checkpoint) fn contains(&self, checkpoint: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.checkpoint_id == checkpoint)
    }

    pub(in crate::checkpoint) fn get(&self, store: &Store, checkpoint: &str) -> Result<Checkpoint> {
        let boundary = self
            .entries
            .iter()
            .find(|entry| entry.checkpoint_id == checkpoint)
            .context("Checkpoint is not in validated ancestry")?;
        let document = store.read_checkpoint(&self.session_id, checkpoint)?;
        ensure!(
            format!("{:x}", Sha256::digest(canonical_bytes(&document)?)) == boundary.sha256,
            "Checkpoint document changed after ancestry validation; retry"
        );
        ensure!(
            format!(
                "{:x}",
                Sha256::digest(canonical_bytes(&store.read_session(&self.session_id)?)?)
            ) == self.session_sha256,
            "Checkpoint session changed after ancestry validation; retry"
        );
        if let Some(head) = &self.head {
            ensure!(
                store.read_head(&self.session_id)? == *head,
                "Checkpoint HEAD changed after ancestry validation; retry"
            );
        } else {
            ensure!(
                safe_path(&store.session_directory(&self.session_id)?.join("HEAD.json"))?.is_none(),
                "Pending checkpoint was published after validation; retry"
            );
        }
        Ok(document)
    }

    pub(in crate::checkpoint) fn current(&self, store: &Store) -> Result<Checkpoint> {
        self.get(
            store,
            &self
                .entries
                .last()
                .context("Empty checkpoint ancestry")?
                .checkpoint_id,
        )
    }

    pub(in crate::checkpoint) fn baseline(&self, store: &Store) -> Result<Checkpoint> {
        self.get(
            store,
            &self
                .entries
                .first()
                .context("Empty checkpoint ancestry")?
                .checkpoint_id,
        )
    }

    pub(in crate::checkpoint) fn iter<'a>(
        &'a self,
        store: &'a Store,
    ) -> impl Iterator<Item = Result<Checkpoint>> + 'a {
        self.entries
            .iter()
            .map(move |entry| self.get(store, &entry.checkpoint_id))
    }
}
