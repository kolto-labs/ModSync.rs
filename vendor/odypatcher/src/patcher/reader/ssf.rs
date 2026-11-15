//! Port of the `[SSFList]` part of `pykotor/tslpatcher/reader.py`
//! (`ConfigReader.load_ssf_list` and `resolve_tslpatcher_ssf_sound`).

use super::{is_ascii_digits, references_traceback, section_not_found, ConfigReader};
use crate::formats::ssf::ENTRY_LABELS;
use crate::patcher::error::{PResult, PatchException};
use crate::patcher::memory::TokenUsage;
use crate::patcher::mods::ssf::{ModificationsSSF, ModifySSF};
use crate::patcher::mods::template::DEFAULT_DESTINATION;
use crate::patcher::mods::PatcherModifications;

/// `int(suffix)` for a run of ASCII digits; oversized ids can never match a
/// stored token, so they saturate.
fn token_number(suffix: &str) -> i32 {
    suffix.parse::<i32>().unwrap_or(i32::MAX)
}

/// The first `count` characters of `text` and the rest (`text[:count]`, `text[count:]`).
fn split_chars(text: &str, count: usize) -> (&str, &str) {
    match text.char_indices().nth(count) {
        Some((index, _)) => text.split_at(index),
        None => (text, ""),
    }
}

impl ConfigReader<'_> {
    /// `load_ssf_list()`: load SSF patches without allowing one invalid entry
    /// to abort the file.
    pub fn load_ssf_list(&mut self) -> PResult<()> {
        let Some(ssf_list_section) = self.get_section_name("SSFList") else {
            self.log.add_note("[SSFList] section missing from ini.");
            return Ok(());
        };

        self.log.add_note("Loading [SSFList] patches from ini...");
        let mut ssf_section_dict = self.section_dict(&ssf_list_section);
        let default_destination =
            ssf_section_dict.pop_or("!DefaultDestination", DEFAULT_DESTINATION);
        let default_source_folder = ssf_section_dict.pop_or("!DefaultSourceFolder", ".");

        let files: Vec<(String, String)> = ssf_section_dict
            .items()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (identifier, file) in files {
            if file.is_empty() {
                continue;
            }
            let Some(ssf_file_section) = self.get_section_name(&file) else {
                self.log.add_warning(format!(
                    "{}{}",
                    section_not_found(&file),
                    references_traceback(&identifier, &file, &ssf_list_section)
                ));
                continue;
            };

            let mut modifications = ModificationsSSF::new(
                &file,
                identifier.to_lowercase().starts_with("replace"),
                Vec::new(),
            );

            let mut file_section_dict = self.section_dict(&ssf_file_section);
            modifications.pop_tslpatcher_vars(
                &mut file_section_dict,
                Some(&default_destination),
                &default_source_folder,
            )?;

            for (name, value) in file_section_dict.items() {
                let sound = match Self::resolve_tslpatcher_ssf_sound(name) {
                    Ok(sound) => sound,
                    Err(_) => {
                        self.log.add_error(format!(
                            "Unknown SSF sound label '{name}' in [{ssf_file_section}]; skipping entry."
                        ));
                        continue;
                    }
                };
                let raw_value = value;
                let (head, suffix) = split_chars(raw_value, 6);
                let stringref = if head.to_lowercase() == "strref" && is_ascii_digits(suffix) {
                    TokenUsage::Tlk(token_number(suffix))
                } else {
                    let (head, suffix) = split_chars(raw_value, 9);
                    if head == "2DAMEMORY" && is_ascii_digits(suffix) {
                        TokenUsage::TwoDA(token_number(suffix))
                    } else {
                        TokenUsage::literal(raw_value)
                    }
                };
                modifications
                    .modifiers
                    .push(ModifySSF::new(sound, stringref));
            }
            self.config.patches_ssf.push(modifications);
        }
        Ok(())
    }

    /// `resolve_tslpatcher_ssf_sound(name)`: the `SSFSound` (as its table
    /// index) for a config label, matched ignoring case; unknown labels are a
    /// `KeyError`.
    pub fn resolve_tslpatcher_ssf_sound(name: &str) -> PResult<u32> {
        let wanted = name.to_lowercase();
        ENTRY_LABELS
            .iter()
            .position(|label| label.to_lowercase() == wanted)
            .map(|index| index as u32)
            .ok_or_else(|| PatchException::key(format!("{name:?}")))
    }
}
