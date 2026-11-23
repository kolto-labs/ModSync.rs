//! Port of the `[HACKList]` and `[CompileList]` parts of
//! `pykotor/tslpatcher/reader.py` (`load_compile_list`, `load_hack_list`).

use super::{is_ascii_digits, references_traceback, section_not_found, ConfigReader};
use crate::patcher::casepath::{case_sensitive_path, safe_isfile};
use crate::patcher::error::PResult;
use crate::patcher::mods::ncs::{HackData, ModificationsNCS};
use crate::patcher::mods::nss::{windows_with_suffix, ModificationsNSS};
use crate::patcher::mods::template::DEFAULT_DESTINATION;
use crate::patcher::mods::PatcherModifications;

/// `int(digits) > 0x7FFFFFFF` for an ASCII-digit string of any length.
fn exceeds_i32(digits: &str) -> bool {
    match digits.trim_start_matches('0').parse::<u128>() {
        Ok(value) => value > 0x7FFF_FFFF,
        Err(_) => !digits.trim_start_matches('0').is_empty(),
    }
}

impl ConfigReader<'_> {
    /// `load_compile_list()`.
    pub fn load_compile_list(&mut self) -> PResult<()> {
        let Some(compilelist_section) = self.get_section_name("CompileList") else {
            self.log.add_note("[CompileList] section missing from ini.");
            return Ok(());
        };

        self.log
            .add_note("Loading [CompileList] patches from ini...");
        let mut compilelist_section_dict = self.section_dict(&compilelist_section);
        let default_destination =
            compilelist_section_dict.pop_or("!DefaultDestination", DEFAULT_DESTINATION);
        let default_source_folder = compilelist_section_dict.pop_or("!DefaultSourceFolder", ".");

        let mut nwnnsscomp_exepath = Some(case_sensitive_path(
            &self
                .patch_data_path
                .join(&default_source_folder)
                .join("nwnnsscomp.exe"),
        ));
        if !nwnnsscomp_exepath
            .as_deref()
            .map(safe_isfile)
            .unwrap_or(false)
        {
            // TSLPatcher default
            nwnnsscomp_exepath = self
                .tslpatchdata_path
                .as_ref()
                .map(|path| case_sensitive_path(&path.join("nwnnsscomp.exe")));
        }

        let mut nwscript_search_folders = vec![self.patch_data_path.join(&default_source_folder)];
        if let Some(tslpatchdata_path) = &self.tslpatchdata_path {
            nwscript_search_folders.push(tslpatchdata_path.clone());
        }

        let entries: Vec<(String, String)> = compilelist_section_dict
            .items()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (identifier, file) in entries {
            let replace = identifier.to_lowercase().starts_with("replace");
            let mut modifications = ModificationsNSS::new(&file, Some(replace));
            modifications.base.destination = default_destination.clone();
            modifications.base.sourcefolder = default_source_folder.clone();

            if let Some(optional_file_section_name) = self.get_section_name(&file) {
                let mut file_section_dict = self.section_dict(&optional_file_section_name);
                file_section_dict.pop("!ReplaceFile");
                modifications.pop_tslpatcher_vars(
                    &mut file_section_dict,
                    Some(&default_destination),
                    &default_source_folder,
                )?;
            }

            modifications.base.saveas = windows_with_suffix(&modifications.base.saveas, ".ncs")?;
            modifications.nwnnsscomp_path = nwnnsscomp_exepath.clone();
            modifications.compiler_flags = self.config.script_compiler_flags.clone();
            modifications.nwscript_search_folders = nwscript_search_folders.clone();
            self.config.patches_nss.push(modifications);
        }
        Ok(())
    }

    /// `load_hack_list()`.
    pub fn load_hack_list(&mut self) -> PResult<()> {
        let Some(hacklist_section) = self.get_section_name("HACKList") else {
            self.log.add_note("[HACKList] section missing from ini.");
            return Ok(());
        };

        self.log.add_note("Loading [HACKList] patches from ini...");
        let mut hacklist_section_dict = self.section_dict(&hacklist_section);
        let default_destination =
            hacklist_section_dict.pop_or("!DefaultDestination", DEFAULT_DESTINATION);
        let default_source_folder = hacklist_section_dict.pop_or("!DefaultSourceFolder", ".");

        let entries: Vec<(String, String)> = hacklist_section_dict
            .items()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (identifier, file) in entries {
            if file.is_empty() {
                continue;
            }
            let mut modifications = ModificationsNCS::new(&file);

            let Some(file_section_name) = self.get_section_name(&file) else {
                self.log.add_error(format!(
                    "{}{}",
                    section_not_found(&file),
                    references_traceback(&identifier, &file, &hacklist_section)
                ));
                continue;
            };

            let mut file_section_dict = self.section_dict(&file_section_name);
            if file_section_dict.is_empty() {
                continue;
            }
            modifications.pop_tslpatcher_vars(
                &mut file_section_dict,
                Some(&default_destination),
                &default_source_folder,
            )?;

            for (offset_str, value_str) in file_section_dict.items() {
                let raw_offset = offset_str.trim();
                let raw_value = value_str.trim();
                if !is_ascii_digits(raw_offset) {
                    self.log.add_warning(format!(
                        "Ignoring invalid HACKList offset '{offset_str}' in [{file_section_name}]."
                    ));
                    continue;
                }

                if exceeds_i32(raw_offset) {
                    self.log.add_warning(format!(
                        "Ignoring out-of-range HACKList offset '{offset_str}' in [{file_section_name}]."
                    ));
                    continue;
                }
                let offset: i32 = raw_offset.trim_start_matches('0').parse().unwrap_or(0);

                let lower_value = raw_value.to_lowercase();
                if lower_value.starts_with("strref") {
                    let token_id = &raw_value[6..];
                    if !is_ascii_digits(token_id) || exceeds_i32(token_id) {
                        self.log.add_warning(format!(
                            "Ignoring invalid HACKList value '{value_str}' at offset {offset_str} in [{file_section_name}]."
                        ));
                        continue;
                    }
                    let token_id: i32 = token_id.trim_start_matches('0').parse().unwrap_or(0);
                    modifications
                        .hackdata
                        .push(HackData::new("StrRef", offset, token_id));
                } else if let Some(token_id) = raw_value.strip_prefix("2DAMEMORY") {
                    if is_ascii_digits(token_id) {
                        if exceeds_i32(token_id) {
                            self.log.add_warning(format!(
                                "Ignoring invalid HACKList value '{value_str}' at offset {offset_str} in [{file_section_name}]."
                            ));
                            continue;
                        }
                        let token_id: i32 = token_id.trim_start_matches('0').parse().unwrap_or(0);
                        modifications
                            .hackdata
                            .push(HackData::new("2DAMEMORY", offset, token_id));
                    } else {
                        modifications
                            .hackdata
                            .push(HackData::new("2DAMEMORY", offset, 0));
                    }
                } else {
                    if !is_ascii_digits(raw_value) || exceeds_i32(raw_value) {
                        self.log.add_warning(format!(
                            "Ignoring invalid HACKList value '{value_str}' at offset {offset_str} in [{file_section_name}]."
                        ));
                        continue;
                    }
                    let value: i32 = raw_value.trim_start_matches('0').parse().unwrap_or(0);
                    modifications
                        .hackdata
                        .push(HackData::new("VALUE", offset, value));
                }
            }

            self.config.patches_ncs.push(modifications);
        }
        Ok(())
    }
}
