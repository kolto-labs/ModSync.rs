//! Port of the `[2DAList]` part of `pykotor/tslpatcher/reader.py`:
//! `load_2da_list`, `discern_2da`, `_read_add_column`, `target_2da`,
//! `cells_2da`, `row_label_2da`, `column_inserts_2da`.
//!
//! At the pinned PyKotor commit every 2DA modifier is built from the raw
//! ordered section entries (`discern_2da`); the `target_2da` / `cells_2da` /
//! `row_label_2da` / `column_inserts_2da` helpers are legacy constructor
//! paths that the INI loader no longer calls. They are ported for parity.

use super::{is_ascii_digits, python_int, references_traceback, section_not_found, ConfigReader};
use crate::patcher::error::{PResult, PatchException};
use crate::patcher::ini::CaseInsensitiveDict;
use crate::patcher::mods::template::DEFAULT_DESTINATION;
use crate::patcher::mods::twoda::{
    store_set, AddColumn2DA, AddRow2DA, ChangeRow2DA, CopyRow2DA, Entry, Modifications2DA,
    Modify2DA, RowValue, Target, TargetType, TargetValue,
};
use crate::patcher::mods::PatcherModifications;

impl ConfigReader<'_> {
    /// `load_2da_list()`.
    ///
    /// A `[2DAList]` entry whose `[file]` section is missing raises `KeyError`
    /// and aborts loading; a modifier whose section is missing gets an empty
    /// entry list.
    pub fn load_2da_list(&mut self) -> PResult<()> {
        let Some(twoda_section_name) = self.get_section_name("2DAList") else {
            self.log.add_note("[2DAList] section missing from ini.");
            return Ok(());
        };

        self.log.add_note("Loading [2DAList] patches from ini...");

        let mut twoda_section_dict = self.section_dict(&twoda_section_name);
        let default_destination =
            twoda_section_dict.pop_or("!DefaultDestination", DEFAULT_DESTINATION);
        let default_source_folder = twoda_section_dict.pop_or("!DefaultSourceFolder", ".");
        let files: Vec<(String, String)> = twoda_section_dict
            .items()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (identifier, file) in files {
            let Some(file_section) = self.get_section_name(&file) else {
                return Err(PatchException::key(format!(
                    "{}{}",
                    section_not_found(&file),
                    references_traceback(&identifier, &file, &twoda_section_name)
                )));
            };

            let mut modifications = Modifications2DA::new(&file, Vec::new());
            let mut file_section_dict = self.section_dict(&file_section);
            modifications.pop_tslpatcher_vars(
                &mut file_section_dict,
                Some(&default_destination),
                &default_source_folder,
            )?;

            self.config.patches_2da.push(modifications);

            let keys: Vec<(String, String)> = file_section_dict
                .items()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            for (key, modification_id) in keys {
                if key.is_empty() || modification_id.is_empty() {
                    continue;
                }

                let modification_ids_dict = match self.get_section_name(&modification_id) {
                    None => CaseInsensitiveDict::new(),
                    Some(next_section_name) => self.section_dict(&next_section_name),
                };
                let Some(manipulation) =
                    self.discern_2da(&key, &modification_id, &modification_ids_dict)
                else {
                    continue;
                };
                if let Some(modifications) = self.config.patches_2da.last_mut() {
                    modifications.modifiers.push(manipulation);
                }
            }
        }
        Ok(())
    }

    /// `discern_2da(key, identifier, modifiers)`: pick the modifier class
    /// from the key's text and hand it the section's entries in order.
    pub fn discern_2da(
        &mut self,
        key: &str,
        identifier: &str,
        modifiers: &CaseInsensitiveDict,
    ) -> Option<Modify2DA> {
        let ordered_entries: Vec<Entry> = modifiers
            .items()
            .map(|(modifier, value)| (modifier.to_string(), value.to_string()))
            .collect();

        if key.contains("AddRow") {
            return Some(Modify2DA::AddRow(AddRow2DA::from_entries(
                identifier,
                ordered_entries,
            )));
        }
        if key.contains("ChangeRow") {
            return Some(Modify2DA::ChangeRow(ChangeRow2DA::from_entries(
                identifier,
                ordered_entries,
            )));
        }
        if key.contains("AddColumn") {
            return Some(Modify2DA::AddColumn(AddColumn2DA::from_entries(
                identifier,
                ordered_entries,
            )));
        }
        if key.contains("CopyRow") {
            return Some(Modify2DA::CopyRow(CopyRow2DA::from_entries(
                identifier,
                ordered_entries,
            )));
        }

        self.log.add_warning(format!(
            "Could not parse key '{key}={identifier}', expecting one of ['AddRow', 'ChangeRow', 'AddColumn', 'CopyRow']"
        ));
        None
    }

    /// `_read_add_column(modifiers, identifier, entries)`: legacy constructor path.
    pub fn read_add_column(
        &mut self,
        modifiers: &mut CaseInsensitiveDict,
        identifier: &str,
        entries: Option<Vec<Entry>>,
    ) -> PResult<AddColumn2DA> {
        let header = modifiers.pop_or("ColumnLabel", "");
        let default = modifiers.pop_or("DefaultValue", "");
        let default = if default != "****" {
            default
        } else {
            String::new()
        };

        let (index_insert, label_insert, store_2da) =
            self.column_inserts_2da(identifier, modifiers)?;
        Ok(AddColumn2DA::new(
            identifier,
            &header,
            &default,
            index_insert,
            label_insert,
            store_2da,
            entries,
        ))
    }

    /// `target_2da(identifier, modifiers)`: legacy constructor path.
    pub fn target_2da(
        &mut self,
        identifier: &str,
        modifiers: &mut CaseInsensitiveDict,
    ) -> PResult<Option<Target>> {
        fn get_target(
            identifier: &str,
            modifiers: &mut CaseInsensitiveDict,
            target_type: TargetType,
            key: &str,
            is_int: bool,
        ) -> PResult<Target> {
            let Some(raw_value) = modifiers.pop(key) else {
                return Err(PatchException::value(format!(
                    "[2DAList] parse error: '{key}' missing from [{identifier}] in ini."
                )));
            };
            let lower_raw_value = raw_value.to_lowercase();
            let value = if lower_raw_value.starts_with("strref")
                && raw_value.len() > "strref".len()
                && is_ascii_digits(&raw_value[6..])
            {
                TargetValue::TlkMemory(python_int(&raw_value[6..])? as i32)
            } else if lower_raw_value.starts_with("2damemory")
                && raw_value.len() > "2damemory".len()
                && is_ascii_digits(&raw_value[9..])
            {
                TargetValue::TwoDAMemory(python_int(&raw_value[9..])? as i32)
            } else if is_int {
                TargetValue::Int(python_int(&raw_value)?)
            } else {
                TargetValue::Str(raw_value)
            };
            Target::new(target_type, value)
        }

        if modifiers.contains("RowIndex") {
            return get_target(
                identifier,
                modifiers,
                TargetType::RowIndex,
                "RowIndex",
                true,
            )
            .map(Some);
        }
        if modifiers.contains("RowLabel") {
            return get_target(
                identifier,
                modifiers,
                TargetType::RowLabel,
                "RowLabel",
                false,
            )
            .map(Some);
        }
        if modifiers.contains("LabelIndex") {
            return get_target(
                identifier,
                modifiers,
                TargetType::LabelColumn,
                "LabelIndex",
                false,
            )
            .map(Some);
        }

        self.log
            .add_warning(format!("No line set to be modified in [{identifier}]."));
        Ok(None)
    }

    /// `cells_2da(identifier, modifiers)`: legacy constructor path;
    /// `(cells, store_2da, store_tlk)`.
    #[allow(clippy::type_complexity)]
    pub fn cells_2da(
        &mut self,
        _identifier: &str,
        modifiers: &CaseInsensitiveDict,
    ) -> PResult<(
        Vec<(String, RowValue)>,
        Vec<(i32, RowValue)>,
        Vec<(i32, RowValue)>,
    )> {
        let mut cells: Vec<(String, RowValue)> = Vec::new();
        let mut store_2da: Vec<(i32, RowValue)> = Vec::new();
        let mut store_tlk: Vec<(i32, RowValue)> = Vec::new();

        for (modifier, value) in modifiers.items() {
            let lower_modifier = modifier.to_lowercase().trim().to_string();
            let lower_value = value.to_lowercase();

            let is_store_2da = lower_modifier.starts_with("2damemory")
                && lower_modifier.len() > "2damemory".len()
                && is_ascii_digits(modifier.get(9..).unwrap_or(""));
            let is_store_tlk = modifier.starts_with("strref")
                && lower_modifier.len() > "strref".len()
                && is_ascii_digits(modifier.get(6..).unwrap_or(""));
            let is_row_label = lower_modifier == "rowlabel" || lower_modifier == "newrowlabel";

            let row_value = if lower_value.starts_with("2damemory") {
                RowValue::TwoDAMemory(python_int(value.get(9..).unwrap_or(""))? as i32)
            } else if lower_value.starts_with("strref") {
                RowValue::TlkMemory(python_int(value.get(6..).unwrap_or(""))? as i32)
            } else if lower_value == "high()" {
                if modifier == "rowlabel" {
                    RowValue::High(None)
                } else {
                    RowValue::High(Some(modifier.to_string()))
                }
            } else if lower_value == "rowindex" {
                RowValue::RowIndex
            } else if lower_value == "rowlabel" {
                RowValue::RowLabel
            } else if is_store_2da || is_store_tlk {
                RowValue::RowCell(value.to_string())
            } else if value == "****" {
                RowValue::Constant(String::new())
            } else {
                RowValue::Constant(value.to_string())
            };

            if is_store_2da {
                store_set(
                    &mut store_2da,
                    python_int(&modifier[9..])? as i32,
                    row_value,
                );
            } else if is_store_tlk {
                store_set(
                    &mut store_tlk,
                    python_int(&modifier[6..])? as i32,
                    row_value,
                );
            } else if !is_row_label {
                store_set(&mut cells, modifier.to_string(), row_value);
            }
        }

        Ok((cells, store_2da, store_tlk))
    }

    /// `row_label_2da(identifier, modifiers)`: legacy constructor path.
    pub fn row_label_2da(
        &mut self,
        _identifier: &str,
        modifiers: &mut CaseInsensitiveDict,
    ) -> PResult<Option<RowValue>> {
        // Python evaluates the default argument first, so both keys are popped.
        let new_row_label = modifiers.pop("NewRowLabel");
        let raw_value = modifiers.pop("RowLabel").or(new_row_label);
        let Some(raw_value) = raw_value else {
            return Ok(None);
        };

        let lower_value = raw_value.to_lowercase().trim().to_string();
        if lower_value == "high()" {
            return Ok(Some(RowValue::High(None)));
        }
        if lower_value.starts_with("2damemory") && is_ascii_digits(raw_value.get(9..).unwrap_or(""))
        {
            return Ok(Some(RowValue::TwoDAMemory(
                python_int(&raw_value[9..])? as i32
            )));
        }
        if lower_value.starts_with("strref") && is_ascii_digits(raw_value.get(6..).unwrap_or("")) {
            return Ok(Some(RowValue::TlkMemory(
                python_int(&raw_value[6..])? as i32
            )));
        }
        Ok(Some(RowValue::Constant(raw_value)))
    }

    /// `column_inserts_2da(identifier, modifiers)`: legacy constructor path;
    /// `(index_insert, label_insert, store_2da)`.
    #[allow(clippy::type_complexity)]
    pub fn column_inserts_2da(
        &mut self,
        _identifier: &str,
        modifiers: &CaseInsensitiveDict,
    ) -> PResult<(
        Vec<(i64, RowValue)>,
        Vec<(String, RowValue)>,
        Vec<(i32, String)>,
    )> {
        let mut index_insert: Vec<(i64, RowValue)> = Vec::new();
        let mut label_insert: Vec<(String, RowValue)> = Vec::new();
        let mut store_2da: Vec<(i32, String)> = Vec::new();

        for (modifier, value) in modifiers.items() {
            let modifier_lowercase = modifier.to_lowercase();
            let value_lowercase = value.to_lowercase();

            let is_store_2da = value_lowercase.starts_with("2damemory");
            let is_store_tlk = value_lowercase.starts_with("strref");

            let row_value = if is_store_2da {
                RowValue::TwoDAMemory(python_int(value.get(9..).unwrap_or(""))? as i32)
            } else if is_store_tlk {
                RowValue::TlkMemory(python_int(value.get(6..).unwrap_or(""))? as i32)
            } else {
                RowValue::Constant(value.to_string())
            };

            let first = modifier.chars().next().unwrap_or_default();
            let rest = &modifier[first.len_utf8()..];
            if modifier_lowercase.starts_with('i') && is_ascii_digits(rest) {
                store_set(&mut index_insert, python_int(rest)?, row_value);
            } else if modifier_lowercase.starts_with('l') {
                store_set(&mut label_insert, rest.to_string(), row_value);
            } else if modifier_lowercase.starts_with("2damemory") {
                store_set(
                    &mut store_2da,
                    python_int(modifier.get(9..).unwrap_or(""))? as i32,
                    value.to_string(),
                );
            }
        }

        Ok((index_insert, label_insert, store_2da))
    }
}
