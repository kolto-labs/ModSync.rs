//! Port of the `[GFFList]` part of `pykotor/tslpatcher/reader.py`:
//! `load_gff_list`, `modify_field_gff`, `_deferred_gff_value`,
//! `_requires_runtime_gff_resolution`, `_resolve_addfield_section`,
//! `add_field_gff`, `_get_addfield_value`, the `field_value_*` helpers and
//! `resolve_tslpatcher_gff_field_type`.

use std::cell::Cell;
use std::sync::Arc;

use super::{is_ascii_digits, python_int, references_traceback, section_not_found, ConfigReader};
use crate::formats::gff::GffStruct;
use crate::patcher::error::{PResult, PatchException};
use crate::patcher::ini::{CaseInsensitiveDict, ConfigParser};
use crate::patcher::mods::gff::{
    base64_decode, decode_hex, exception_str, AddFieldGFF, AddFieldGFFReference,
    AddStructToListGFF, FieldValueKind, GffFieldType, GffPath, GffValue, LocalizedStringDelta,
    Memory2DAModifierGFF, ModificationsGFF, ModifyFieldGFF, ModifyGFF,
};
use crate::patcher::mods::template::DEFAULT_DESTINATION;
use crate::patcher::mods::PatcherModifications;

thread_local! {
    /// Nesting depth of `add_field_gff`, standing in for Python's recursion
    /// limit: a self-referencing `AddField` chain raises `RecursionError`
    /// (which the per-operation handlers do not catch) instead of
    /// overflowing the stack.
    static ADD_FIELD_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Python (reader.py:829 `add_field_gff`) hits its 1000-frame recursion limit
/// after roughly 300 nested sections. Each Rust level costs ~10 KiB of stack in
/// debug builds, so 400 levels overflowed a default 2 MiB thread before the
/// guard could raise; 100 levels fit with room to spare and still exceed any
/// real INI's nesting.
const MAX_ADD_FIELD_DEPTH: usize = 100;

struct DepthGuard;

impl DepthGuard {
    fn enter() -> PResult<Self> {
        let depth = ADD_FIELD_DEPTH.with(|d| d.get());
        if depth >= MAX_ADD_FIELD_DEPTH {
            return Err(PatchException::new(
                "RecursionError",
                "maximum recursion depth exceeded",
            ));
        }
        ADD_FIELD_DEPTH.with(|d| d.set(depth + 1));
        Ok(Self)
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        ADD_FIELD_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// `int(digits)` for a token id; Python's unbounded integers saturate here.
fn token_id(digits: &str) -> i32 {
    digits.parse::<i32>().unwrap_or(i32::MAX)
}

/// `key.startswith("2DAMEMORY") and key[9:].isascii() and key[9:].isdigit()`.
fn is_memory_token(text: &str) -> bool {
    text.starts_with("2DAMEMORY") && is_ascii_digits(&text[9..])
}

/// `float(text)` with Python's grammar (underscores between digits, `inf`,
/// `nan`, exponents, surrounding whitespace).
fn python_float(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut cleaned = String::with_capacity(trimmed.len());
    for (index, &b) in bytes.iter().enumerate() {
        if b == b'_' {
            let before = index.checked_sub(1).map(|i| bytes[i]).unwrap_or(b' ');
            let after = bytes.get(index + 1).copied().unwrap_or(b' ');
            if !(before.is_ascii_digit() && after.is_ascii_digit()) {
                return None;
            }
            continue;
        }
        cleaned.push(b as char);
    }
    cleaned.parse::<f64>().ok()
}

impl ConfigReader<'_> {
    /// `load_gff_list()`: load GFF patches while keeping each field operation
    /// independent.
    pub fn load_gff_list(&mut self) -> PResult<()> {
        let Some(gff_list_section) = self.get_section_name("GFFList") else {
            self.log.add_note("[GFFList] section missing from ini.");
            return Ok(());
        };

        self.log.add_note("Loading [GFFList] patches from ini...");
        let mut gff_section_dict = self.section_dict(&gff_list_section);
        let default_destination =
            gff_section_dict.pop_or("!DefaultDestination", DEFAULT_DESTINATION);
        let default_source_folder = gff_section_dict.pop_or("!DefaultSourceFolder", ".");
        let mut shared_ini: Option<Arc<ConfigParser>> = None;

        let entries: Vec<(String, String)> = gff_section_dict
            .items()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (identifier, file) in entries {
            let Some(file_section_name) = self.get_section_name(&file) else {
                self.log.add_error(format!(
                    "{}{}",
                    section_not_found(&file),
                    references_traceback(&identifier, &file, &gff_list_section)
                ));
                continue;
            };

            self.config
                .patches_gff
                .push(ModificationsGFF::new(&file, Some(false), Vec::new()));
            let index = self.config.patches_gff.len() - 1;

            let mut file_section_dict = self.section_dict(&file_section_name);
            self.config.patches_gff[index].pop_tslpatcher_vars(
                &mut file_section_dict,
                Some(&default_destination),
                &default_source_folder,
            )?;

            let operations: Vec<(String, String)> = file_section_dict
                .items()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            for (key, value) in operations {
                match self.parse_gff_operation(&file_section_name, &key, &value, &mut shared_ini) {
                    Ok(modifier) => self.config.patches_gff[index].modifiers.push(modifier),
                    Err(exc) if matches!(exc.kind, "KeyError" | "TypeError" | "ValueError") => {
                        // A bare key prints as Python's `None`.
                        let shown_value = match self
                            .ini
                            .section(&file_section_name)
                            .and_then(|s| s.get(&key))
                        {
                            Some(None) => "None".to_string(),
                            _ => value.clone(),
                        };
                        self.log.add_error(format!(
                            "Unable to parse GFF operation '{key}={shown_value}' in [{file_section_name}]: {}",
                            exception_str(&exc)
                        ));
                    }
                    Err(exc) => return Err(exc),
                }
            }
        }
        Ok(())
    }

    /// The body of the per-key `try` block in `load_gff_list`.
    fn parse_gff_operation(
        &mut self,
        file_section_name: &str,
        key: &str,
        raw_value: &str,
        shared_ini: &mut Option<Arc<ConfigParser>>,
    ) -> PResult<ModifyGFF> {
        if key.starts_with("AddField") {
            if Self::requires_runtime_gff_resolution(raw_value) {
                let ini = shared_ini
                    .get_or_insert_with(|| Arc::new(self.ini.clone()))
                    .clone();
                return Ok(ModifyGFF::AddFieldReference(AddFieldGFFReference::new(
                    file_section_name,
                    Self::deferred_gff_value(raw_value, None),
                    ini,
                )));
            }
            return self.resolve_addfield_section(raw_value);
        }

        if is_memory_token(key) {
            let dest = token_id(&key[9..]);
            if raw_value == "!FieldPath" {
                return Ok(ModifyGFF::Memory2DA(Memory2DAModifierGFF::new(
                    file_section_name,
                    GffPath::root(),
                    dest,
                    None,
                    false,
                    false,
                )));
            }
            if is_memory_token(raw_value) {
                return Ok(ModifyGFF::Memory2DA(Memory2DAModifierGFF::new(
                    file_section_name,
                    GffPath::root(),
                    dest,
                    Some(token_id(&raw_value[9..])),
                    false,
                    false,
                )));
            }
        }

        Ok(ModifyGFF::ModifyField(Self::modify_field_gff(
            file_section_name,
            key,
            raw_value,
        )))
    }

    /// `modify_field_gff(identifier, key, str_value)`: create a direct GFF
    /// field operation without committing to a destination type.
    pub fn modify_field_gff(identifier: &str, key: &str, str_value: &str) -> ModifyFieldGFF {
        let mut value: FieldValueKind = Self::deferred_gff_value(str_value, None);
        let lower_key = key.to_lowercase();
        let mut key = key.to_string();
        let mut path_token_id: Option<i32> = None;

        if let Some(selector) = lower_key.find("(strref)") {
            value = FieldValueKind::constant(GffValue::LocDelta(LocalizedStringDelta::new(Some(
                Self::deferred_gff_value(str_value, Some(GffFieldType::Int32)),
            ))));
            key = cut_key(&key, &lower_key, selector);
        } else if let Some(selector) = key.find("(lang") {
            if key.ends_with(')') {
                let substring = &key[selector + 5..key.len() - 1];
                if is_ascii_digits(substring) {
                    let substring_id = token_id(substring);
                    let mut locstring = LocalizedStringDelta::new(None);
                    let field_value =
                        Self::deferred_gff_value(str_value, Some(GffFieldType::String));
                    if Self::requires_runtime_gff_resolution(str_value) {
                        locstring.set_field_value(substring_id, field_value);
                    } else {
                        let (language, gender) = LocalizedStringDelta::substring_pair(substring_id);
                        locstring.set_data(
                            language,
                            gender,
                            &Self::normalize_tslpatcher_crlf(str_value),
                        );
                    }
                    value = FieldValueKind::constant(GffValue::LocDelta(locstring));
                    key = key[..selector].to_string();
                }
            }
        }

        if is_memory_token(&key) {
            path_token_id = Some(token_id(&key[9..]));
            key = String::new();
        }

        ModifyFieldGFF::new(GffPath::new(&key), value, identifier, path_token_id, false)
    }

    /// `_deferred_gff_value(raw_value, field_type)`: a memory token, else a
    /// `FieldValueRaw` carrying the best-effort parsed preview.
    pub fn deferred_gff_value(raw_value: &str, field_type: Option<GffFieldType>) -> FieldValueKind {
        if let Some(memory_value) = Self::field_value_from_memory(raw_value) {
            return memory_value;
        }

        let parsed_value: Option<FieldValueKind> = match field_type {
            Some(ft) => Self::field_value_from_type(raw_value, ft).ok().flatten(),
            None => Self::field_value_from_unknown(raw_value).ok(),
        };

        let preview = parsed_value
            .as_ref()
            .and_then(FieldValueKind::stored)
            .cloned()
            .unwrap_or_else(|| GffValue::Str(raw_value.to_string()));
        FieldValueKind::raw_with_preview(raw_value, preview)
    }

    /// `_requires_runtime_gff_resolution(raw_value)`.
    pub fn requires_runtime_gff_resolution(raw_value: &str) -> bool {
        raw_value.starts_with("2DAMEMORY")
            || (raw_value.len() >= 6
                && raw_value.is_char_boundary(6)
                && raw_value[..6].eq_ignore_ascii_case("strref")
                && is_ascii_digits(&raw_value[6..]))
    }

    /// `_resolve_addfield_section(section)`: parse the named `AddField`
    /// section; a missing section is a `KeyError`.
    pub fn resolve_addfield_section(&mut self, section: &str) -> PResult<ModifyGFF> {
        let Some(section_name) = self.get_section_name(section) else {
            return Err(PatchException::key(section_not_found(section)));
        };
        let ini_data = self.section_dict(&section_name);
        self.add_field_gff(&section_name, &ini_data, None)
    }

    /// `add_field_gff(identifier, ini_data, current_path)`: parse one
    /// `AddField` section while preserving nested operation order.
    pub fn add_field_gff(
        &mut self,
        identifier: &str,
        ini_data: &CaseInsensitiveDict,
        current_path: Option<GffPath>,
    ) -> PResult<ModifyGFF> {
        let _guard = DepthGuard::enter()?;

        // A bare `FieldType` line is `None` in Python, which is "not set".
        let bare_field_type = matches!(
            self.ini
                .section(identifier)
                .and_then(|s| s.get("FieldType")),
            Some(None)
        ) && ini_data.get("FieldType") == Some("");
        let raw_field_type = if bare_field_type {
            None
        } else {
            ini_data.get("FieldType")
        };
        let Some(raw_field_type) = raw_field_type else {
            return Err(PatchException::value(format!(
                "FieldType must be set in [{identifier}]."
            )));
        };
        let field_type = Self::resolve_tslpatcher_gff_field_type(raw_field_type)?;

        let label = ini_data.get("Label").unwrap_or("").trim().to_string();
        let has_explicit_path = ini_data.contains("Path");
        let raw_path = ini_data.get("Path").unwrap_or("");
        let path = GffPath::new(raw_path.trim());
        let relative_path = current_path.is_some() && !has_explicit_path;

        let value = Self::get_addfield_value(ini_data, field_type, identifier);
        let mut modifiers: Vec<ModifyGFF> = Vec::new();

        let items: Vec<(String, String)> = ini_data
            .items()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (key, raw_iterated_value) in items {
            if is_memory_token(&key) {
                let dest = token_id(&key[9..]);
                if raw_iterated_value == "ListIndex" {
                    modifiers.push(ModifyGFF::Memory2DA(Memory2DAModifierGFF::new(
                        identifier,
                        GffPath::root(),
                        dest,
                        None,
                        true,
                        true,
                    )));
                } else if raw_iterated_value == "!FieldPath" {
                    modifiers.push(ModifyGFF::Memory2DA(Memory2DAModifierGFF::new(
                        identifier,
                        GffPath::root(),
                        dest,
                        None,
                        true,
                        false,
                    )));
                } else if is_memory_token(&raw_iterated_value) {
                    modifiers.push(ModifyGFF::Memory2DA(Memory2DAModifierGFF::new(
                        identifier,
                        GffPath::root(),
                        dest,
                        Some(token_id(&raw_iterated_value[9..])),
                        true,
                        false,
                    )));
                }
                continue;
            }

            if key.starts_with("AddField") && key.len() > "AddField".len() {
                let Some(next_section_name) = self.get_section_name(&raw_iterated_value) else {
                    self.log.add_error(format!(
                        "{}{}",
                        section_not_found(&raw_iterated_value),
                        references_traceback(&key, &raw_iterated_value, identifier)
                    ));
                    continue;
                };
                let next_data = self.section_dict(&next_section_name);
                match self.add_field_gff(&next_section_name, &next_data, Some(GffPath::root())) {
                    Ok(nested_modifier) => modifiers.push(nested_modifier),
                    Err(exc) if matches!(exc.kind, "KeyError" | "TypeError" | "ValueError") => {
                        self.log.add_error(format!(
                            "Unable to parse nested AddField section [{next_section_name}]: {}",
                            exception_str(&exc)
                        ));
                        continue;
                    }
                    Err(exc) => return Err(exc),
                }
            }
        }

        if field_type == GffFieldType::Struct && label.is_empty() {
            return Ok(ModifyGFF::AddStructToList(AddStructToListGFF::new(
                identifier,
                value,
                path,
                None,
                modifiers,
                relative_path,
            )));
        }
        if label.is_empty() {
            return Err(PatchException::value(format!(
                "Label must be set for FieldType={} in [{identifier}].",
                field_type.name()
            )));
        }
        Ok(ModifyGFF::AddField(AddFieldGFF::new(
            identifier,
            &label,
            field_type,
            value,
            path,
            modifiers,
            relative_path,
        )))
    }

    /// `_get_addfield_value(ini_section_dict, field_type, identifier)`: an
    /// `AddField` value that retains the original INI text for apply-time
    /// validation.
    pub fn get_addfield_value(
        ini_section_dict: &CaseInsensitiveDict,
        field_type: GffFieldType,
        _identifier: &str,
    ) -> FieldValueKind {
        match field_type {
            GffFieldType::LocalizedString => {
                Self::field_value_from_localized_string(ini_section_dict)
            }
            GffFieldType::List => FieldValueKind::constant(GffValue::List(Vec::new())),
            GffFieldType::Struct => {
                let raw_type_id = match ini_section_dict.get("TypeId") {
                    Some("") | None => "0",
                    Some(text) => text,
                };
                if raw_type_id.to_lowercase() == "listindex" {
                    return FieldValueKind::ListIndex(GffValue::Str("listindex".to_string()));
                }
                let mut preview = GffStruct::new();
                preview.type_id = if is_ascii_digits(raw_type_id) {
                    raw_type_id.parse::<u32>().unwrap_or(0)
                } else {
                    0
                };
                FieldValueKind::raw_with_preview(raw_type_id, GffValue::Struct(preview))
            }
            _ if ini_section_dict.contains("Value") => Self::deferred_gff_value(
                ini_section_dict.get("Value").unwrap_or(""),
                Some(field_type),
            ),
            GffFieldType::ResRef => {
                FieldValueKind::raw_with_preview("", GffValue::ResRef(String::new()))
            }
            GffFieldType::String => {
                FieldValueKind::raw_with_preview("", GffValue::Str(String::new()))
            }
            GffFieldType::Binary => {
                FieldValueKind::raw_with_preview("", GffValue::Bytes(Vec::new()))
            }
            _ => FieldValueKind::raw(""),
        }
    }

    /// `field_value_from_localized_string(ini_section_dict)`: a
    /// localized-string delta with token substitution deferred to apply time.
    pub fn field_value_from_localized_string(
        ini_section_dict: &CaseInsensitiveDict,
    ) -> FieldValueKind {
        let raw_stringref = match ini_section_dict.get("StrRef") {
            Some("") | None => "-1",
            Some(text) => text,
        };
        let mut delta = LocalizedStringDelta::new(Some(Self::deferred_gff_value(
            raw_stringref,
            Some(GffFieldType::Int32),
        )));

        for (substring, text) in ini_section_dict.items() {
            let Some(suffix) = substring.strip_prefix("lang") else {
                continue;
            };
            if !is_ascii_digits(suffix) {
                continue;
            }
            let substring_id = token_id(suffix);
            if Self::requires_runtime_gff_resolution(text) {
                delta.set_field_value(
                    substring_id,
                    Self::deferred_gff_value(text, Some(GffFieldType::String)),
                );
            } else {
                let (language, gender) = LocalizedStringDelta::substring_pair(substring_id);
                delta.set_data(language, gender, &Self::normalize_tslpatcher_crlf(text));
            }
        }

        FieldValueKind::constant(GffValue::LocDelta(delta))
    }

    /// `field_value_from_memory(raw_value)`: a GFF memory token only when the
    /// token grammar is valid.
    pub fn field_value_from_memory(raw_value: &str) -> Option<FieldValueKind> {
        if raw_value.len() >= 6
            && raw_value.is_char_boundary(6)
            && raw_value[..6].eq_ignore_ascii_case("strref")
            && is_ascii_digits(&raw_value[6..])
        {
            return Some(FieldValueKind::MemoryTLK(token_id(&raw_value[6..])));
        }
        if is_memory_token(raw_value) {
            return Some(FieldValueKind::Memory2DA(token_id(&raw_value[9..])));
        }
        None
    }

    /// `field_value_from_unknown(raw_value)`: guess the type of an INI value
    /// (`EntryList/0/RepliesList/0/TypeId=5`).
    pub fn field_value_from_unknown(raw_value: &str) -> PResult<FieldValueKind> {
        // StrRef or 2DAMemory
        if let Some(field_value_memory) = Self::field_value_from_memory(raw_value) {
            return Ok(field_value_memory);
        }

        // Int
        if let Ok(int_value) = python_int(raw_value) {
            return Ok(FieldValueKind::constant(GffValue::Int(i128::from(
                int_value,
            ))));
        }

        // Float
        let parsed_float_str = Self::normalize_tslpatcher_float(raw_value);
        if let Some(float_value) = python_float(&parsed_float_str) {
            return Ok(FieldValueKind::constant(GffValue::Float(float_value)));
        }

        let num_pipe_seps = raw_value.matches('|').count();

        // String
        if num_pipe_seps == 0 {
            return Ok(FieldValueKind::constant(GffValue::Str(
                Self::normalize_tslpatcher_crlf(raw_value),
            )));
        }

        // Vector
        let mut components: Vec<f64> = Vec::new();
        let mut not_a_vector = false;
        for x in parsed_float_str.split('|') {
            match python_float(x) {
                Some(component) => components.push(component),
                None => {
                    not_a_vector = true;
                    break;
                }
            }
        }

        // String containing the character '|'
        if not_a_vector {
            return Ok(FieldValueKind::constant(GffValue::Str(
                Self::normalize_tslpatcher_crlf(raw_value),
            )));
        }

        // Three floats
        if num_pipe_seps == 2 {
            return Ok(FieldValueKind::constant(GffValue::Vector3([
                components[0],
                components[1],
                components[2],
            ])));
        }

        // Four floats
        if num_pipe_seps == 3 {
            return Ok(FieldValueKind::constant(GffValue::Vector4([
                components[0],
                components[1],
                components[2],
                components[3],
            ])));
        }

        Err(PatchException::value(format!(
            "Cannot determine type/value from '{raw_value}'"
        )))
    }

    /// `field_value_from_type(raw_value, field_type)`: parse an INI value for
    /// a known type; `Ok(None)` for the container and localized types.
    pub fn field_value_from_type(
        raw_value: &str,
        field_type: GffFieldType,
    ) -> PResult<Option<FieldValueKind>> {
        if let Some(field_value_memory) = Self::field_value_from_memory(raw_value) {
            return Ok(Some(field_value_memory));
        }

        let float_of = |axis: &str| -> PResult<f64> {
            python_float(&Self::normalize_tslpatcher_float(axis)).ok_or_else(|| {
                PatchException::value(format!(
                    "could not convert string to float: {}",
                    crate::patcher::mods::gff::python_str_repr(axis)
                ))
            })
        };

        let value = match field_type {
            GffFieldType::ResRef => {
                GffValue::ResRef(crate::patcher::mods::gff::make_resref(raw_value)?)
            }
            GffFieldType::String => GffValue::Str(Self::normalize_tslpatcher_crlf(raw_value)),
            ft if ft.is_integer() => GffValue::Int(i128::from(python_int(raw_value)?)),
            ft if ft.is_float() => GffValue::Float(float_of(raw_value)?),
            GffFieldType::Vector3 => {
                let components = raw_value
                    .split('|')
                    .map(float_of)
                    .collect::<PResult<Vec<f64>>>()?;
                if components.len() != 3 {
                    return Err(PatchException::new(
                        "TypeError",
                        format!(
                            "Vector3.__init__() takes 4 positional arguments but {} were given",
                            components.len() + 1
                        ),
                    ));
                }
                GffValue::Vector3([components[0], components[1], components[2]])
            }
            GffFieldType::Vector4 => {
                let components = raw_value
                    .split('|')
                    .map(float_of)
                    .collect::<PResult<Vec<f64>>>()?;
                if components.len() != 4 {
                    return Err(PatchException::new(
                        "TypeError",
                        format!(
                            "Vector4.__init__() takes 5 positional arguments but {} were given",
                            components.len() + 1
                        ),
                    ));
                }
                GffValue::Vector4([components[0], components[1], components[2], components[3]])
            }
            GffFieldType::Binary => {
                let stripped = raw_value.trim();
                if stripped.chars().all(|c| c == '0' || c == '1') {
                    let bytes = raw_value
                        .as_bytes()
                        .chunks(8)
                        .map(|chunk| {
                            std::str::from_utf8(chunk)
                                .ok()
                                .and_then(|text| u8::from_str_radix(text, 2).ok())
                                .ok_or_else(|| {
                                    PatchException::value(format!(
                                        "invalid literal for int() with base 2: {}",
                                        crate::patcher::mods::gff::python_str_repr(
                                            &String::from_utf8_lossy(chunk)
                                        )
                                    ))
                                })
                        })
                        .collect::<PResult<Vec<u8>>>()?;
                    GffValue::Bytes(bytes)
                } else if stripped.to_lowercase().starts_with("0x") {
                    let mut hex_string = raw_value.get(2..).unwrap_or("").to_string();
                    if hex_string.len() % 2 == 1 {
                        hex_string.insert(0, '0');
                    }
                    let bytes = decode_hex(hex_string.trim()).ok_or_else(|| {
                        PatchException::value("non-hexadecimal number found in fromhex() arg")
                    })?;
                    GffValue::Bytes(bytes)
                } else {
                    let bytes = base64_decode(raw_value, false).ok_or_else(|| {
                        PatchException::value(format!(
                            "The raw value for the binary field specified was invalid: '{raw_value}'"
                        ))
                    })?;
                    GffValue::Bytes(bytes)
                }
            }
            _ => return Ok(None),
        };

        Ok(Some(FieldValueKind::constant(value)))
    }

    /// `resolve_tslpatcher_gff_field_type(field_type_num_str)`: the
    /// `FieldType=` names, matched ignoring case; unknown names are a
    /// `KeyError` carrying the lower-cased name.
    pub fn resolve_tslpatcher_gff_field_type(field_type_num_str: &str) -> PResult<GffFieldType> {
        Ok(match field_type_num_str.to_lowercase().as_str() {
            "binary" => GffFieldType::Binary, // HoloPatcher only.
            "byte" => GffFieldType::UInt8,
            "char" => GffFieldType::Int8,
            "word" => GffFieldType::UInt16,
            "short" => GffFieldType::Int16,
            "dword" => GffFieldType::UInt32,
            "int" => GffFieldType::Int32,
            "int64" => GffFieldType::Int64,
            "float" => GffFieldType::Single,
            "double" => GffFieldType::Double,
            "exostring" => GffFieldType::String,
            "resref" => GffFieldType::ResRef,
            "exolocstring" => GffFieldType::LocalizedString,
            "position" => GffFieldType::Vector3,
            "orientation" => GffFieldType::Vector4,
            "struct" => GffFieldType::Struct,
            "list" => GffFieldType::List,
            other => return Err(PatchException::key(other.to_string())),
        })
    }
}

/// `key[:selector]` where `selector` was found in `key.lower()`.
fn cut_key(key: &str, lower_key: &str, selector: usize) -> String {
    if key.is_char_boundary(selector) && key.len() == lower_key.len() {
        return key[..selector].to_string();
    }
    let count = lower_key[..selector].chars().count();
    key.chars().take(count).collect()
}
