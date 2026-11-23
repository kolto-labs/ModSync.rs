use super::*;

pub(super) fn decode_slot(
    snapshot: &Path,
    pin: ArchivePin,
    scratch: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
    native: String,
) -> Result<Slot> {
    let maximum = expansion_limit(&pin.sha256);
    let mut bounded = options.clone();
    bounded.scratch_dir = Some(scratch.to_owned());
    bounded.max_expanded_bytes = Some(options.max_expanded_bytes.unwrap_or(maximum).min(maximum));
    let (listed, total) =
        archive::list_original_with_size(snapshot, Path::new(&pin.filename), &bounded, cancelled)?;
    ensure!(
        listed.len() <= MAX_ENTRIES && total <= maximum,
        "Original program archive exceeds bounds"
    );
    let tree = tempfile::Builder::new()
        .prefix("modsync-program-tree-")
        .tempdir_in(scratch)?;
    let mut members: BTreeMap<String, Member> = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    let mut explicit = BTreeSet::new();
    for item in listed {
        check(cancelled)?;
        let name = item.path.trim_end_matches('/').to_owned();
        literal(&name)?;
        ensure!(
            explicit.insert(crate::unicode_case::ordinal_key(&name)),
            "Duplicate original archive member"
        );
        let mut paths: Vec<_> = name
            .match_indices('/')
            .map(|(i, _)| (name[..i].to_owned(), true))
            .collect();
        paths.push((name, item.is_directory));
        for (path, directory) in paths {
            ensure!(
                Path::new(&path).components().count() <= 64,
                "Original archive path depth exceeds bound"
            );
            let key = crate::unicode_case::ordinal_key(&path);
            if let Some(old) = aliases.insert(key, path.clone()) {
                ensure!(old == path, "Original archive aliases");
            }
            if let Some(old) = members.get(&path) {
                ensure!(
                    old.directory && directory,
                    "Original archive file/directory collision"
                );
            } else {
                ensure!(
                    members.len() < MAX_ENTRIES,
                    "Original canonical inventory exceeds bound"
                );
                members.insert(
                    path.clone(),
                    Member {
                        path,
                        directory,
                        bytes: 0,
                        sha256: String::new(),
                    },
                );
            }
        }
    }
    for m in members.values().filter(|m| m.directory) {
        fs::create_dir_all(tree.path().join(literal(&m.path)?))?;
    }
    let mut seen = BTreeSet::new();
    let mut expanded = 0u64;
    for item in archive::unpack(snapshot, Path::new(&pin.filename), &bounded, cancelled)? {
        check(cancelled)?;
        let name = item
            .path
            .to_str()
            .context("Non UTF8 original archive member")?
            .replace('\\', "/");
        let member = members
            .get_mut(&name)
            .context("Decoded original archive member not listed")?;
        ensure!(
            !member.directory && seen.insert(name.clone()),
            "Duplicate original decoded member"
        );
        let mut input = crate::paths::open_regular_read(item.file.path())?;
        let before = input.metadata()?;
        ordinary(&before, false)?;
        ensure!(
            before.len() <= MAX_MEMBER,
            "Decoded original member exceeds bound"
        );
        let target = tree.path().join(literal(&name)?);
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut copied = 0u64;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            check(cancelled)?;
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            ensure!(
                copied <= before.len(),
                "Decoded original member grew during snapshot"
            );
            digest.update(&buffer[..n]);
            output.write_all(&buffer[..n])?;
        }
        let after = input.metadata()?;
        ensure!(
            copied == before.len()
                && after.len() == before.len()
                && after.modified().ok() == before.modified().ok(),
            "Decoded original member changed during snapshot"
        );
        expanded = expanded.checked_add(copied).context("Expansion overflow")?;
        ensure!(expanded <= maximum, "Original expansion exceeds bound");
        member.bytes = copied;
        member.sha256 = format!("{:x}", digest.finalize());
    }
    ensure!(
        expanded == total && seen.len() == members.values().filter(|m| !m.directory).count(),
        "Original archive decoding incomplete"
    );
    Ok(Slot {
        tree,
        members,
        archive: pin,
        native,
    })
}
