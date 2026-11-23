//! Port of the `[TLKList]` part of `pykotor/tslpatcher/reader.py`
//! (`ConfigReader.load_tlk_list`).

use std::path::PathBuf;

use super::{references_traceback, section_not_found, ConfigReader};
use crate::patcher::casepath::case_sensitive_path;
use crate::patcher::error::{PResult, PatchException};
use crate::patcher::mods::tlk::{
    resref_from_text, MergeTLK, ModificationsTLK, ModifyTLK, TlkModifier,
};
use crate::patcher::mods::PatcherModifications;

/// `parse_tlk_index(raw_value, description)`: non-negative ASCII decimal
/// digits only. `None` stands for a key written without a value.
fn parse_tlk_index(raw_value: Option<&str>, description: &str) -> PResult<i32> {
    let invalid = || {
        PatchException::value(format!(
            "Invalid {description} '{}': expected non-negative ASCII decimal digits",
            raw_value.unwrap_or("None")
        ))
    };
    let Some(text) = raw_value else {
        return Err(invalid());
    };
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    // Python's int is unbounded; anything past i32 cannot address a table
    // and fails later as a nonexistent stringref, like an oversized index.
    Ok(text.parse::<i32>().unwrap_or(i32::MAX))
}

/// `raw[6:] if raw.lower().startswith("strref") else raw`.
fn strip_strref_prefix(raw: &str) -> &str {
    if raw.to_lowercase().starts_with("strref") {
        raw.get(6..).unwrap_or(raw)
    } else {
        raw
    }
}

/// Where a `[TLKList]` key failed: with its own message (`syntax_error_caught`),
/// or with a `ValueError` the caller rewraps as `Could not parse ...`.
enum TlkKeyFailure {
    Syntax(PatchException),
    Other(PatchException),
}

impl ConfigReader<'_> {
    /// `load_tlk_list()`.
    pub fn load_tlk_list(&mut self) -> PResult<()> {
        let Some(tlk_list_section) = self.get_section_name("tlklist") else {
            self.log.add_note("[TLKList] section missing from ini.");
            return Ok(());
        };

        self.log.add_note("Loading [TLKList] patches from ini...");
        let mut tlk_list_edits = self.section_dict(&tlk_list_section);

        let default_destination =
            tlk_list_edits.pop_or("!DefaultDestination", ModificationsTLK::DEFAULT_DESTINATION);
        let default_sourcefolder = tlk_list_edits.pop_or("!DefaultSourceFolder", ".");
        self.config.patches_tlk.pop_tslpatcher_vars(
            &mut tlk_list_edits,
            Some(&default_destination),
            &default_sourcefolder,
        )?;

        // Position of the single MergeTLK in `modifiers`, once created.
        let mut source_tlk_merge: Option<usize> = None;
        let raw_section = self
            .ini
            .section(&tlk_list_section)
            .cloned()
            .unwrap_or_default();
        let edits: Vec<(String, String)> = tlk_list_edits
            .items()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        for (key, value) in edits {
            // A key written without `=` is `None` in Python; it prints as such.
            let raw_value: Option<String> = match raw_section.get(&key) {
                Some(None) => None,
                _ => Some(value.clone()),
            };
            let shown_value = raw_value.clone().unwrap_or_else(|| "None".to_string());
            let outcome = self.load_tlk_key(
                &key,
                raw_value.as_deref(),
                &shown_value,
                &tlk_list_section,
                &default_destination,
                &default_sourcefolder,
                &mut source_tlk_merge,
            );
            match outcome {
                Ok(()) => {}
                Err(TlkKeyFailure::Syntax(err)) => return Err(err),
                Err(TlkKeyFailure::Other(err)) if err.kind == "ValueError" => {
                    return Err(PatchException::value(format!(
                        "Could not parse '{key}={shown_value}' in [TLKList]"
                    )));
                }
                Err(TlkKeyFailure::Other(err)) => return Err(err),
            }
        }
        Ok(())
    }

    /// One iteration of the `for key, value in tlk_list_edits.items()` loop.
    #[allow(clippy::too_many_arguments)]
    fn load_tlk_key(
        &mut self,
        key: &str,
        value: Option<&str>,
        shown_value: &str,
        tlk_list_section: &str,
        default_destination: &str,
        default_sourcefolder: &str,
        source_tlk_merge: &mut Option<usize>,
    ) -> Result<(), TlkKeyFailure> {
        let lower_key = key.to_lowercase();
        let replace_file = lower_key.starts_with("replace");
        let append_file = lower_key.starts_with("append");

        if lower_key.starts_with("strref") {
            let token_id =
                parse_tlk_index(key.get(6..), "StrRef token").map_err(TlkKeyFailure::Other)?;
            let source_stringref =
                parse_tlk_index(value, "source TLK index").map_err(TlkKeyFailure::Other)?;
            let index = match *source_tlk_merge {
                Some(index) => index,
                None => {
                    let path =
                        self.tlk_source_path(&self.config.patches_tlk.base.sourcefile.clone());
                    self.config
                        .patches_tlk
                        .modifiers
                        .push(TlkModifier::Merge(MergeTLK::new(path)));
                    let index = self.config.patches_tlk.modifiers.len() - 1;
                    *source_tlk_merge = Some(index);
                    index
                }
            };
            if let Some(TlkModifier::Merge(merge)) =
                self.config.patches_tlk.modifiers.get_mut(index)
            {
                merge.add_mapping(token_id, source_stringref);
            }
            Ok(())
        } else if replace_file || append_file {
            let Some(next_section_name) = self.get_section_name(value.unwrap_or("")) else {
                return Err(TlkKeyFailure::Syntax(PatchException::value(format!(
                    "{}{}",
                    section_not_found(shown_value),
                    references_traceback(key, shown_value, tlk_list_section)
                ))));
            };

            let mut next_section_dict = self.section_dict(&next_section_name);
            self.config
                .patches_tlk
                .pop_tslpatcher_vars(
                    &mut next_section_dict,
                    Some(default_destination),
                    default_sourcefolder,
                )
                .map_err(TlkKeyFailure::Other)?;

            // The raw section is walked, so a `!` key left in it fails to parse.
            let lines: Vec<(String, Option<String>)> = self
                .ini
                .section(&next_section_name)
                .map(|section| section.items().to_vec())
                .unwrap_or_default();
            for (raw_dialog_tlk_index, raw_mod_tlk_index) in lines {
                let dialog_index_text = strip_strref_prefix(&raw_dialog_tlk_index);
                let mod_index_text = raw_mod_tlk_index.as_deref().map(strip_strref_prefix);
                let dialog_tlk_index =
                    parse_tlk_index(Some(dialog_index_text), "destination TLK index")
                        .map_err(TlkKeyFailure::Other)?;
                let mod_tlk_index = parse_tlk_index(mod_index_text, "source TLK index")
                    .map_err(TlkKeyFailure::Other)?;
                self.process_tlk_entries(
                    &next_section_name,
                    dialog_tlk_index,
                    mod_tlk_index,
                    replace_file,
                );
            }
            Ok(())
        } else if lower_key.contains('\\') || lower_key.contains('/') {
            let delimiter = if lower_key.contains('\\') { '\\' } else { '/' };
            let parts: Vec<&str> = lower_key.split(delimiter).collect();
            let (token_id_str, property_name) = match parts.as_slice() {
                [token_id_str, property_name] => (*token_id_str, *property_name),
                other => {
                    // `token_id_str, property_name = lower_key.split(delimiter)` unpack failure.
                    let count = other.len();
                    let message = if count > 2 {
                        format!("too many values to unpack (expected 2, got {count})")
                    } else {
                        format!("not enough values to unpack (expected 2, got {count})")
                    };
                    return Err(TlkKeyFailure::Other(PatchException::value(message)));
                }
            };
            let token_id = parse_tlk_index(Some(token_id_str), "destination TLK index")
                .map_err(TlkKeyFailure::Other)?;

            if property_name == "text" {
                let mut modifier = ModifyTLK::new(token_id, true);
                modifier.set_text(value.unwrap_or(""));
                self.config
                    .patches_tlk
                    .modifiers
                    .push(TlkModifier::Modify(modifier));
                Ok(())
            } else if property_name == "sound" {
                let mut modifier = ModifyTLK::new(token_id, true);
                let resref = resref_from_text(value.unwrap_or("")).map_err(TlkKeyFailure::Other)?;
                modifier.set_sound(&resref);
                self.config
                    .patches_tlk
                    .modifiers
                    .push(TlkModifier::Modify(modifier));
                Ok(())
            } else {
                Err(TlkKeyFailure::Syntax(PatchException::value(format!(
                    "Invalid [TLKList] syntax: '{key}={shown_value}'! Expected '{key}' to be one of ['Sound', 'Text']"
                ))))
            }
        } else {
            Err(TlkKeyFailure::Syntax(PatchException::value(format!(
                "Invalid syntax found in [TLKList] '{key}={shown_value}'! Expected '{key}' to be one of ['AppendFile', 'ReplaceFile', '!SourceFile', 'StrRef', 'Text', 'Sound']"
            ))))
        }
    }

    /// `CaseAwarePath.get_case_sensitive_path(patch_data_path / patches_tlk.sourcefolder / filename)`.
    fn tlk_source_path(&self, filename: &str) -> PathBuf {
        case_sensitive_path(
            &self
                .patch_data_path
                .join(&self.config.patches_tlk.base.sourcefolder)
                .join(filename),
        )
    }

    /// `process_tlk_entries(tlk_filename, dialog_tlk_index, mod_tlk_index, is_replacement=...)`.
    fn process_tlk_entries(
        &mut self,
        tlk_filename: &str,
        dialog_tlk_index: i32,
        mod_tlk_index: i32,
        is_replacement: bool,
    ) {
        let mut modifier = ModifyTLK::new(dialog_tlk_index, is_replacement);
        modifier.mod_index = mod_tlk_index;
        modifier.tlk_filepath = Some(self.tlk_source_path(tlk_filename));
        self.config
            .patches_tlk
            .modifiers
            .push(TlkModifier::Modify(modifier));
    }
}
