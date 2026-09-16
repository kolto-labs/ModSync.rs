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
            .entry(path.to_lowercase())
            .or_insert_with(|| (path, Vec::new()));
        if history.last().is_some_and(|old| {
            old.component_guid
                .eq_ignore_ascii_case(&writer.component_guid)
        }) {
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
            report.limitations.push("External program and patcher writes cannot be simulated; this report excludes their effects.".into());
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
        report
    }
}
