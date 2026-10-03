//! File-write histories collected by the shared install simulation.
use crate::Component;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Writer {
    pub component_guid: String,
    pub component_name: String,
    pub instruction_index: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileConflict {
    pub destination_path: String,
    pub writers: Vec<Writer>,
    pub winner_component_guid: String,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub conflicts: Vec<FileConflict>,
    pub conflict_counts_by_component: BTreeMap<String, usize>,
    pub analyzed_component_count: usize,
    pub limitations: Vec<String>,
}

/// Immutable desktop/CLI view of an already-computed report. No simulation or
/// filesystem reads occur while building it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    pub summary: String,
    pub rows: Vec<ConflictRow>,
    pub components: Vec<ComponentCount>,
    pub limitations: Vec<String>,
    pub report_json: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRow {
    pub destination_path: String,
    pub writers: Vec<WriterRow>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterRow {
    pub component_guid: String,
    pub component_name: String,
    pub instruction_index: usize,
    pub is_winner: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentCount {
    pub component_guid: String,
    pub component_name: String,
    pub conflicting_paths: usize,
}

impl Presentation {
    pub fn from_report(report: &Report) -> serde_json::Result<Self> {
        let summary = if report.conflicts.is_empty() {
            if report.analyzed_component_count == 0 {
                "No selected components to analyze.".into()
            } else {
                format!(
                    "No simulated file conflicts among {} selected components.",
                    report.analyzed_component_count
                )
            }
        } else {
            format!(
                "{} conflicting file paths across {} selected components.",
                report.conflicts.len(),
                report.analyzed_component_count
            )
        };
        let mut names = BTreeMap::new();
        let mut rows: Vec<_> = report
            .conflicts
            .iter()
            .map(|conflict| ConflictRow {
                destination_path: conflict.destination_path.clone(),
                writers: conflict
                    .writers
                    .iter()
                    .map(|writer| {
                        names
                            .entry(crate::guid::key_ascii(&writer.component_guid))
                            .or_insert_with(|| writer.component_name.clone());
                        WriterRow {
                            component_guid: writer.component_guid.clone(),
                            component_name: writer.component_name.clone(),
                            instruction_index: writer.instruction_index,
                            is_winner: crate::guid::eq_ascii(
                                &writer.component_guid,
                                &conflict.winner_component_guid,
                            ),
                        }
                    })
                    .collect(),
            })
            .collect();
        rows.sort_by(|a, b| {
            crate::unicode_case::ordinal_cmp(&a.destination_path, &b.destination_path)
                .then(a.destination_path.cmp(&b.destination_path))
        });
        let mut components: Vec<_> = report
            .conflict_counts_by_component
            .iter()
            .map(|(guid, count)| ComponentCount {
                component_guid: guid.clone(),
                component_name: names
                    .get(&crate::guid::key_ascii(guid))
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| guid.clone()),
                conflicting_paths: *count,
            })
            .collect();
        components.sort_by(|a, b| {
            b.conflicting_paths
                .cmp(&a.conflicting_paths)
                .then(a.component_name.cmp(&b.component_name))
                .then(a.component_guid.cmp(&b.component_guid))
        });
        Ok(Self {
            summary,
            rows,
            components,
            limitations: report.limitations.clone(),
            report_json: serde_json::to_string_pretty(report)?,
        })
    }
}

#[derive(Default)]
pub(crate) struct Tracker {
    pub current: Option<Writer>,
    histories: BTreeMap<String, (String, Vec<Writer>)>,
    count: usize,
    pub external_effects: bool,
}
impl Tracker {
    pub fn begin(&mut self, component: &Component) {
        self.count += 1;
        self.current = Some(Writer {
            component_guid: component.guid.clone(),
            component_name: component.name.clone(),
            instruction_index: 0,
        });
    }
    pub fn write(&mut self, relative: &str) {
        let Some(writer) = &self.current else { return };
        let path = format!("<<kotorDirectory>>/{}", relative.replace('\\', "/"));
        let (_, history) = self
            .histories
            .entry(crate::unicode_case::ordinal_key(&path))
            .or_insert_with(|| (path, Vec::new()));
        if history
            .last()
            .is_some_and(|old| crate::guid::eq_ascii(&old.component_guid, &writer.component_guid))
        {
            history.pop();
        }
        history.push(writer.clone());
    }
    pub fn finish(self) -> Report {
        let mut report = Report {
            analyzed_component_count: self.count,
            ..Report::default()
        };
        if self.external_effects {
            report.limitations.push("External program writes and binary patch effects are excluded; the report includes only the supported patcher InstallList writes.".into());
        }
        for (_, (destination_path, writers)) in self.histories {
            if writers.len() < 2 {
                continue;
            }
            for writer in &writers {
                *report
                    .conflict_counts_by_component
                    .entry(writer.component_guid.clone())
                    .or_default() += 1;
            }
            report.conflicts.push(FileConflict {
                destination_path,
                winner_component_guid: writers.last().unwrap().component_guid.clone(),
                writers,
            });
        }
        // The reference sorts by OrdinalIgnoreCase UTF-16 order, not by key bytes.
        report.conflicts.sort_by(|a, b| {
            crate::unicode_case::ordinal_cmp(&a.destination_path, &b.destination_path)
        });
        report
    }
}
