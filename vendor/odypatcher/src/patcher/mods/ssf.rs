//! Port of `pykotor/tslpatcher/mods/ssf.py`, together with the parts of
//! `pykotor/resource/formats/ssf` (`SSF`, `SSFSound`, `SSFBinaryReader`,
//! `SSFBinaryWriter`) the patch needs.
//!
//! PyKotor keeps the table's entry count (at least 28, growing to the highest
//! slot set), the bytes between the header and the table, and any trailing
//! bytes, and writes them all back. The crate's `SsfFile` normalises to a
//! 40-entry table at offset 12, so the table is modelled locally here.

use super::super::config::Game;
use super::super::error::{PResult, PatchException};
use super::super::logger::PatchLogger;
use super::super::memory::{PatcherMemory, TokenUsage};
use super::template::{ModBase, PatchOutput, PatcherModifications};

/// `len(SSFSound)`.
pub const SSF_SOUND_COUNT: usize = 40;
/// Engine sound events; later slots are optional stored data.
const SSF_ENGINE_ENTRY_COUNT: usize = 28;

/// `SSFSound` member names in enum order (`sound.name`).
pub const SSF_SOUND_NAMES: [&str; SSF_SOUND_COUNT] = [
    "BATTLE_CRY_1",
    "BATTLE_CRY_2",
    "BATTLE_CRY_3",
    "BATTLE_CRY_4",
    "BATTLE_CRY_5",
    "BATTLE_CRY_6",
    "SELECT_1",
    "SELECT_2",
    "SELECT_3",
    "ATTACK_GRUNT_1",
    "ATTACK_GRUNT_2",
    "ATTACK_GRUNT_3",
    "PAIN_GRUNT_1",
    "PAIN_GRUNT_2",
    "LOW_HEALTH",
    "DEAD",
    "CRITICAL_HIT",
    "TARGET_IMMUNE",
    "LAY_MINE",
    "DISARM_MINE",
    "BEGIN_STEALTH",
    "BEGIN_SEARCH",
    "BEGIN_UNLOCK",
    "UNLOCK_FAILED",
    "UNLOCK_SUCCESS",
    "SEPARATED_FROM_PARTY",
    "REJOINED_PARTY",
    "POISONED",
    "UNKNOWN_29",
    "UNKNOWN_30",
    "UNKNOWN_31",
    "UNKNOWN_32",
    "UNKNOWN_33",
    "UNKNOWN_34",
    "UNKNOWN_35",
    "UNKNOWN_36",
    "UNKNOWN_37",
    "UNKNOWN_38",
    "UNKNOWN_39",
    "UNKNOWN_40",
];

/// `SSFSound(index).name`.
pub fn ssf_sound_name(sound: u32) -> &'static str {
    SSF_SOUND_NAMES
        .get(sound as usize)
        .copied()
        .unwrap_or("UNKNOWN")
}

/// `SSF`: the data stored in a soundset file. Stringrefs are kept as the
/// `uint32` on disk; `0xFFFFFFFF` is PyKotor's `-1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ssf {
    sounds: [u32; SSF_SOUND_COUNT],
    entry_count: usize,
    padding: Vec<u8>,
    trailing_data: Vec<u8>,
}

impl Default for Ssf {
    fn default() -> Self {
        Self::new()
    }
}

impl Ssf {
    /// `SSF()`: every stringref `-1`, 28 entries.
    pub fn new() -> Self {
        Self {
            sounds: [u32::MAX; SSF_SOUND_COUNT],
            entry_count: SSF_ENGINE_ENTRY_COUNT,
            padding: Vec::new(),
            trailing_data: Vec::new(),
        }
    }

    /// `ssf.get(sound)` as the on-disk `uint32` (`-1` is `u32::MAX`).
    pub fn get(&self, sound: u32) -> Option<u32> {
        self.sounds.get(sound as usize).copied()
    }

    /// `ssf.get(sound)` as PyKotor's signed value.
    pub fn get_signed(&self, sound: u32) -> Option<i64> {
        self.get(sound).map(|value| {
            if value == u32::MAX {
                -1
            } else {
                i64::from(value)
            }
        })
    }

    /// `ssf.set_data(sound, stringref)`.
    pub fn set_data(&mut self, sound: u32, stringref: u32) {
        let index = sound as usize;
        if index < SSF_SOUND_COUNT {
            self.sounds[index] = stringref;
            self.entry_count = self.entry_count.max(index + 1);
        }
    }

    /// How many entries the table writes (`_entry_count`).
    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// `SSFBinaryReader(source).load()`.
    pub fn from_bytes(bytes: &[u8]) -> PResult<Self> {
        let size = bytes.len();
        if size < 12 {
            return Err(PatchException::new(
                "OSError",
                "This operation would exceed the streams boundaries.",
            ));
        }
        if &bytes[0..4] != b"SSF " {
            return Err(PatchException::value(
                "Attempted to load an invalid SSF was loaded.",
            ));
        }
        if &bytes[4..8] != b"V1.1" {
            return Err(PatchException::value(
                "The supplied SSF file version is not supported.",
            ));
        }
        let sounds_offset = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        if sounds_offset < 12 || sounds_offset + SSF_ENGINE_ENTRY_COUNT * 4 > size {
            return Err(PatchException::value(
                "The SSF data offset must address all 28 sound entries.",
            ));
        }
        let mut ssf = Ssf::new();
        ssf.padding = bytes[12..sounds_offset].to_vec();
        let entry_count = SSF_SOUND_COUNT.min((size - sounds_offset) / 4);
        for index in 0..entry_count {
            let at = sounds_offset + index * 4;
            let value =
                u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            ssf.set_data(index as u32, value);
        }
        ssf.trailing_data = bytes[sounds_offset + entry_count * 4..].to_vec();
        Ok(ssf)
    }

    /// `bytes_ssf(ssf)`: `SSFBinaryWriter(ssf).write()` to memory.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            12 + self.padding.len() + self.entry_count * 4 + self.trailing_data.len(),
        );
        out.extend_from_slice(b"SSF ");
        out.extend_from_slice(b"V1.1");
        out.extend_from_slice(&((12 + self.padding.len()) as u32).to_le_bytes());
        out.extend_from_slice(&self.padding);
        for index in 0..self.entry_count {
            out.extend_from_slice(&self.sounds[index].to_le_bytes());
        }
        out.extend_from_slice(&self.trailing_data);
        out
    }
}

/// `_parse_ssf_integer(value)`: `$hex` up to `0xFFFFFFFF`, `4294967295`, or a
/// signed decimal in `int32` range masked to `uint32`; anything else is `None`.
pub fn parse_ssf_integer(value: &str) -> Option<u32> {
    let text = value.trim();
    if let Some(hex) = text.strip_prefix('$') {
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return u64::from_str_radix(hex, 16)
                .ok()
                .filter(|number| *number <= 0xFFFF_FFFF)
                .map(|number| number as u32);
        }
        return None;
    }
    if text == "4294967295" {
        return Some(0xFFFF_FFFF);
    }
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        let number = text.parse::<i64>().ok()?;
        if (-0x8000_0000..=0x7FFF_FFFF).contains(&number) {
            return Some((number & 0xFFFF_FFFF) as u32);
        }
    }
    None
}

/// `ModifySSF(sound, stringref)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifySSF {
    /// The `SSFSound` slot, as its table index (0..=39).
    pub sound: u32,
    /// The value to write.
    pub stringref: TokenUsage,
}

impl ModifySSF {
    /// `ModifySSF(sound, stringref)`.
    pub fn new(sound: u32, stringref: TokenUsage) -> Self {
        Self { sound, stringref }
    }

    /// `_resolve(memory)`: the raw value as `str(value)`.
    fn resolve(&self, memory: &PatcherMemory) -> PResult<String> {
        match &self.stringref {
            TokenUsage::Tlk(token_id) => Ok(memory
                .memory_str
                .get(token_id)
                .copied()
                .unwrap_or(0)
                .to_string()),
            TokenUsage::TwoDA(token_id) => {
                let raw_value = format!("2DAMEMORY{token_id}");
                if memory.memory_2da.is_empty() {
                    return Ok(raw_value);
                }
                Ok(lookup_2da(memory, Some(*token_id)))
            }
            TokenUsage::NoToken(_) => {
                let raw_value = self.stringref.value(memory)?.as_str().to_string();
                if let Some(suffix) = raw_value.strip_prefix("2DAMEMORY") {
                    if memory.memory_2da.is_empty() {
                        return Ok(raw_value);
                    }
                    let token_id =
                        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                            suffix.parse::<i32>().ok()
                        } else {
                            Some(1)
                        };
                    return Ok(lookup_2da(memory, token_id));
                }
                Ok(raw_value)
            }
        }
    }

    /// `apply(ssf, memory, logger)`: true when the table changed.
    pub fn apply(&self, ssf: &mut Ssf, memory: &PatcherMemory, logger: &mut PatchLogger) -> bool {
        let name = ssf_sound_name(self.sound);
        let raw = match self.resolve(memory) {
            Ok(raw) => raw,
            Err(exc) => {
                logger.add_warning(format!(
                    "Unable to resolve SSF value for {name}: {}",
                    exc.message
                ));
                return false;
            }
        };
        let Some(value) = parse_ssf_integer(&raw) else {
            logger.add_warning(format!("Invalid SSF value for {name}; skipping entry."));
            return false;
        };
        // `-1 if value == 0xFFFFFFFF else value`: both are u32::MAX on disk.
        if ssf.get(self.sound) == Some(value) {
            return false;
        }
        ssf.set_data(self.sound, value);
        true
    }
}

/// `memory.memory_2da.get(token_id, "")` after clamping the token to
/// `1..=max(memory.memory_2da)`; `None` stands for an unparsable huge id.
fn lookup_2da(memory: &PatcherMemory, token_id: Option<i32>) -> String {
    let highest = memory.memory_2da.keys().next_back().copied().unwrap_or(0);
    let token_id = match token_id {
        Some(id) if (1..=highest).contains(&id) => id,
        _ => 1,
    };
    memory
        .memory_2da
        .get(&token_id)
        .map(|value| value.as_str().to_string())
        .unwrap_or_default()
}

/// `ModificationsSSF`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModificationsSSF {
    /// Shared attributes.
    pub base: ModBase,
    /// Entries to set, in INI order.
    pub modifiers: Vec<ModifySSF>,
}

impl ModificationsSSF {
    /// `ModificationsSSF(filename, replace, modifiers)`.
    pub fn new(filename: &str, replace: bool, modifiers: Vec<ModifySSF>) -> Self {
        Self {
            base: ModBase::new(filename, Some(replace)),
            modifiers,
        }
    }

    /// `apply(ssf, memory, logger, game)`: true when any entry changed.
    pub fn apply(
        &self,
        ssf: &mut Ssf,
        memory: &PatcherMemory,
        logger: &mut PatchLogger,
        _game: Game,
    ) -> bool {
        let mut changed = false;
        for modifier in &self.modifiers {
            changed = modifier.apply(ssf, memory, logger) || changed;
        }
        changed
    }
}

impl PatcherModifications for ModificationsSSF {
    fn base(&self) -> &ModBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ModBase {
        &mut self.base
    }

    fn patch_resource(
        &mut self,
        source: &[u8],
        memory: &mut PatcherMemory,
        logger: &mut PatchLogger,
        game: Game,
    ) -> PResult<PatchOutput> {
        let mut ssf = Ssf::from_bytes(source)?;
        self.apply(&mut ssf, memory, logger, game);
        Ok(PatchOutput::Bytes(ssf.to_bytes()))
    }

    fn class_name(&self) -> &'static str {
        "ModificationsSSF"
    }
}
