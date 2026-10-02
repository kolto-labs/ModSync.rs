//! Bounded InstallList presence simulation. This never runs a patcher and does not
//! model binary patch operations. Unknown payloads retain an explicit VFS marker.
use super::{Content, Engine, Event};
use crate::paths::relative;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_INI_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EFFECTS: usize = 100_000;

/// Case-insensitive names; insertion order matters for namespaces and install writes.
#[derive(Default)]
struct Ini(BTreeMap<String, Vec<(String, String)>>);

impl Ini {
    fn parse(text: &str, cancelled: &std::sync::atomic::AtomicBool) -> Result<Self> {
        let mut ini = Self::default();
        let mut positions = BTreeMap::<(String, String), usize>::new();
        let mut section = None;
        for line in text
            .trim_start_matches('\u{feff}')
            .split(['\r', '\n'])
            .map(str::trim)
        {
            ensure!(
                !cancelled.load(std::sync::atomic::Ordering::Relaxed),
                "Patcher simulation cancelled"
            );
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let name = line[1..line.len() - 1].trim().to_lowercase();
                ini.0.entry(name.clone()).or_default();
                section = Some(name);
            } else if let Some(section) = &section
                && let Some((key, value)) = line.split_once('=')
            {
                let key = key.trim().to_lowercase();
                if key.is_empty() {
                    continue;
                }
                let fields = ini.0.get_mut(section).expect("section inserted");
                let identity = (section.clone(), key.clone());
                if let Some(index) = positions.get(&identity) {
                    fields[*index].1 = value.trim().to_owned();
                } else {
                    ensure!(
                        positions.len() < MAX_EFFECTS,
                        "Patcher INI exceeds {MAX_EFFECTS} keys"
                    );
                    positions.insert(identity, fields.len());
                    fields.push((key, value.trim().to_owned()));
                }
            }
        }
        Ok(ini)
    }

    fn section(&self, name: &str) -> Option<&[(String, String)]> {
        self.0.get(&name.to_lowercase()).map(Vec::as_slice)
    }

    fn field(&self, section: &str, name: &str) -> Option<&str> {
        self.section(section)?
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

impl Engine {
    fn simulation_ini(&self, path: &Path) -> Result<Ini> {
        self.settings.roots.check(path)?;
        let content = &self
            .files
            .get(&self.path_policy.key(path))
            .context("Selected patcher INI is missing")?
            .1;
        let mut source = content.open()?.take(MAX_INI_BYTES + 1);
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            self.check_cancelled()?;
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
        ensure!(
            bytes.len() as u64 <= MAX_INI_BYTES,
            "Patcher simulation INI exceeds 8 MiB"
        );
        let text = super::patcher_prepare::decode(&bytes)?;
        Ini::parse(&text, &self.settings.cancelled)
    }

    fn simulation_configuration(&self, base: &Path, arguments: &str) -> Result<Option<PathBuf>> {
        let arguments = arguments.trim();
        let data = if base
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("tslpatchdata"))
        {
            base.to_path_buf()
        } else {
            base.join("tslpatchdata")
        };
        // Named INI uses the same data root as the production staging branch.
        if !arguments.is_empty() && arguments.parse::<usize>().is_err() {
            let name = relative(arguments, false)?;
            ensure!(
                name.components().count() == 1
                    && name
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ini")),
                "Patcher Arguments must be an INI filename or namespace index"
            );
            let selected = self.actual(&data.join(name));
            self.settings.roots.check(&selected)?;
            ensure!(
                self.files.contains_key(&self.path_policy.key(&selected)),
                "Missing selected patcher INI: {}",
                selected.display()
            );
            return Ok(Some(selected));
        }
        let index = arguments.parse::<usize>().unwrap_or(0);
        let namespaces = [base.join("namespaces.ini"), data.join("namespaces.ini")]
            .into_iter()
            .map(|path| self.actual(&path))
            .find(|path| self.files.contains_key(&self.path_policy.key(path)));
        if let Some(namespaces) = namespaces {
            let ini = self.simulation_ini(&namespaces)?;
            let entries = ini
                .section("Namespaces")
                .context("Patcher namespace index needs a [Namespaces] section")?;
            let (_, section) = entries
                .get(index)
                .context("Patcher namespace index is out of range")?;
            let filename = ini
                .field(section, "IniName")
                .filter(|value| !value.is_empty())
                .context("Selected patcher namespace has no IniName")?;
            let folder = ini.field(section, "DataPath").unwrap_or("");
            let filename = relative(filename, false)?;
            ensure!(
                filename.file_name().is_some(),
                "Selected namespace INI has no filename"
            );
            let selected = self.actual(
                &namespaces
                    .parent()
                    .expect("INI has parent")
                    .join(relative(folder, false)?)
                    .join(filename),
            );
            self.settings.roots.check(&selected)?;
            ensure!(
                self.files.contains_key(&self.path_policy.key(&selected)),
                "Selected namespace INI is missing: {}",
                selected.display()
            );
            return Ok(Some(selected));
        }
        ensure!(index == 0, "Patcher namespace index needs namespaces.ini");
        Ok([base.join("changes.ini"), data.join("changes.ini")]
            .into_iter()
            .map(|path| self.actual(&path))
            .find(|path| self.files.contains_key(&self.path_policy.key(path))))
    }

    pub(super) fn simulate_patcher(
        &mut self,
        base: &Path,
        destination: &Path,
        arguments: &str,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        ensure!(
            !self.install,
            "Patcher simulation is only available during virtual execution"
        );
        self.settings.roots.check(base)?;
        self.settings.roots.check(destination)?;
        let Some(configuration) = self.simulation_configuration(base, arguments)? else {
            self.validation_warning(
                "PatcherSimulation",
                "No changes.ini found; no InstallList effects inferred".into(),
                base,
            );
            return Ok(());
        };
        let ini = self.simulation_ini(&configuration)?;
        let source_root = configuration
            .parent()
            .context("Patcher INI has no parent")?;
        let mut effects = Vec::new();
        if let Some(folders) = ini.section("InstallList") {
            for (section, folder) in folders {
                if !section.starts_with("install_folder") || folder.is_empty() {
                    continue;
                }
                let Some(files) = ini.section(section) else {
                    continue;
                };
                let folder = relative(folder, false)?;
                for (operation, filename) in files {
                    self.check_cancelled()?;
                    if !(operation.starts_with("file") || operation.starts_with("replace"))
                        || filename.is_empty()
                    {
                        continue;
                    }
                    let filename = relative(filename, false)?;
                    ensure!(
                        filename.file_name().is_some(),
                        "Patcher InstallList entry has no filename"
                    );
                    let source = self.actual(&source_root.join(&filename));
                    let target = self.actual(&destination.join(&folder).join(&filename));
                    self.settings.roots.check(&source)?;
                    self.settings.roots.check(&target)?;
                    ensure!(
                        effects.len() < MAX_EFFECTS,
                        "Patcher InstallList exceeds {MAX_EFFECTS} effects"
                    );
                    effects.push((source, target));
                }
            }
        }
        let count = effects.len();
        let mut unknown = 0;
        for (source, target) in effects {
            self.check_cancelled()?;
            let content = self
                .files
                .get(&self.path_policy.key(&source))
                .map(|(_, content)| content.clone())
                .unwrap_or(Content::Unknown);
            if matches!(content, Content::Unknown) {
                unknown += 1;
                self.validation_warning(
                    "PatcherSimulation",
                    "InstallList predicts file presence but payload content is unknown".into(),
                    &target,
                );
            }
            self.put(&target, content, true)?;
        }
        emit(Event {
            component: "Patcher".into(),
            step: 0,
            message: format!(
                "Simulated {count} InstallList files ({unknown} unknown payloads) from {}; binary patch effects remain unmodeled",
                configuration.display()
            ),
        });
        Ok(())
    }
}
