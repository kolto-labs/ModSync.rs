//! Closed, private candidate byte materializer. No filesystem, process, model
//! mutation, component approval, or installed-success authority is exposed.
use crate::{Component, Guide};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};

const DATA: &str = include_str!("k1_widescreen_profile.json");
const DATA_SHA: &str = "5828e338a0f06e65f5ff9b0a019f64221d7090ad40d6362f397f5905609ad2e4";
pub(crate) const PROFILE_ID: &str = "k1-1920x1080-1024-interface-letterbox-off-movies-on-v1";
const EXE_SIZE: u64 = 4042752;
const BASELINE: &str = "34e6d971c034222a417995d8e1e8fdd9f8781795c9c289bd86c499a439f34c88";
const EDITABLE: &str = "761f9466f456a83909036baebb5c43167d722387be66e54617ba20a8c49e9886";
const UNIWS: &str = "5fd092af0c1d914df814832d181e868a40517b1ad0fc15d01eef56039878f514";
const HRMENUS: &str = "63698d8aba343621591646b03e2a8fafd4a170befbe9dce65fc4f873b5429bf0";
const FOURGB: &str = "d739e41e79f89ee98e666f66deb22a8f85da5b44908c4e9fcfcb24fea723f959";
const GUI_PREFIX: &str = "16-by-9/gui.1920x1080/";
const LIMIT: usize = 32 * 1024 * 1024;
const GAPS: &[&str] = &["actual-end-to-end-CI-install-checkpoint-recovery-and-receipt-proof"];

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    schema: u32,
    source_guide_revision: String,
    source_guide_sha256: String,
    source_plan_sha256: String,
    profile: String,
    vendor_output_equivalence_proven: bool,
    vendor_output_equivalence_scope: String,
    uniws_vendor_output_equivalence_proven: bool,
    uniws_vendor_witness: Value,
    hrmenus_vendor_output_equivalence_proven: bool,
    fourgb_vendor_output_equivalence_proven: bool,
    hrmenus_vendor_witness: Value,
    fourgb_vendor_witness: Value,
    ini_creation: IniCreation,
    source_components: Vec<Owner>,
    archives: Vec<ArchivePin>,
    assets: Vec<Asset>,
    source_provenance: BTreeMap<String, String>,
    uniws_edits: Vec<Edit>,
    hrmenus_edits: Vec<Edit>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IniCreation {
    path: String,
    baseline_present: bool,
    baseline_engine_sha: String,
    absence_evidence_sha256: String,
    policy: String,
    encoding: String,
    line_endings: String,
    content: String,
    bytes: u64,
    sha256: String,
    game_launch_proven: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    original_index: usize,
    raw: Value,
    canonical: Value,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArchivePin {
    role: String,
    filename: String,
    sha256: String,
    bytes: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    archive: String,
    path: String,
    sha256: String,
    bytes: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    offset: usize,
    expected: String,
    replacement: String,
}

/// Source documents must be original inputs, before binding or review clearing.
pub(crate) struct SourceContext<'a> {
    pub(crate) guide_revision: &'a str,
    pub(crate) guide_body: &'a [u8],
    pub(crate) source_plan: &'a [u8],
    pub(crate) raw_components: &'a [Value],
    pub(crate) canonical_components: &'a [Component],
}
/// Exact selected members decoded by the caller. This module verifies every byte
/// against the compiled pin; archive/member names never become filesystem paths.
pub(crate) struct MemberInput<'a> {
    pub(crate) path: &'a str,
    pub(crate) bytes: &'a [u8],
}
pub(crate) struct ArchiveInput<'a> {
    pub(crate) role: &'a str,
    pub(crate) filename: &'a str,
    pub(crate) bytes: &'a [u8],
    pub(crate) members: &'a [MemberInput<'a>],
}
pub(crate) struct CandidateContract {
    profile: Profile,
    members: BTreeMap<(String, String), Vec<u8>>,
}
/// Materialized candidates only. Possession grants no component completion or
/// permission to write these bytes into a game tree.
pub(crate) struct CandidateOutput {
    pub(crate) files: BTreeMap<String, Vec<u8>>,
    pub(crate) receipt: Value,
}

fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "K1 materialization cancelled"
    );
    Ok(())
}
fn digest(bytes: &[u8], cancelled: &AtomicBool) -> Result<String> {
    ensure!(bytes.len() <= LIMIT, "K1 byte input exceeds bound");
    let mut hash = Sha256::new();
    for chunk in bytes.chunks(65536) {
        check(cancelled)?;
        hash.update(chunk);
    }
    check(cancelled)?;
    Ok(format!("{:x}", hash.finalize()))
}
fn verify(bytes: &[u8], size: u64, sha: &str, cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        bytes.len() as u64 == size && digest(bytes, cancelled)? == sha,
        "K1 exact byte identity differs"
    );
    Ok(())
}
fn safe(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 240
            && path.is_ascii()
            && !path.contains(['\\', ':'])
            && !path.bytes().any(|b| b < 32 || b == 127)
            && path.split('/').all(|part| !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.contains(['*', '?', '"', '<', '>', '|'])),
        "Unsafe K1 member path"
    );
    Ok(())
}

/// The only constructor: fixed compiled authority, exact original full owner
/// contexts, explicit profile, source document pins, and actual acquired bytes.
pub(crate) fn admit(
    explicit_profile: &str,
    source: SourceContext<'_>,
    inputs: &[ArchiveInput<'_>],
    cancelled: &AtomicBool,
) -> Result<CandidateContract> {
    check(cancelled)?;
    ensure!(
        explicit_profile == PROFILE_ID,
        "K1 requires the explicit fixed profile"
    );
    verify(DATA.as_bytes(), DATA.len() as u64, DATA_SHA, cancelled)?;
    let profile: Profile = serde_json::from_str(DATA)?;
    ensure!(
        profile.schema == 1
            && profile.profile == PROFILE_ID
            && profile.vendor_output_equivalence_proven
            && profile.vendor_output_equivalence_scope
                == "exact-fixed-public-input-UniWS-HRMenus-FourGB-executable-chain-only"
            && profile.hrmenus_vendor_output_equivalence_proven
            && profile.fourgb_vendor_output_equivalence_proven
            && profile.hrmenus_vendor_witness["input_sha256"] == UNIWS
            && profile.hrmenus_vendor_witness["output_sha256"] == HRMENUS
            && profile.fourgb_vendor_witness["input_sha256"] == HRMENUS
            && profile.fourgb_vendor_witness["output_sha256"] == FOURGB
            && profile.uniws_vendor_output_equivalence_proven
            && profile.uniws_vendor_witness["input_sha256"] == EDITABLE
            && profile.uniws_vendor_witness["output_sha256"] == UNIWS
            && profile.uniws_vendor_witness["output_bytes"] == EXE_SIZE
            && profile.uniws_vendor_witness["vendor_success_dialog_observed"] == true
            && profile.uniws_vendor_witness["final_quiescent"] == true
            && profile.uniws_vendor_witness["exact_candidate_match"] == true
            && profile.source_components.len() == 3
            && profile.archives.len() == 5
            && profile.assets.len() == 90
            && profile.uniws_edits.len() == 9
            && profile.hrmenus_edits.len() == 10,
        "K1 compiled profile scope differs"
    );
    ensure!(
        source.guide_revision == profile.source_guide_revision,
        "K1 guide revision differs"
    );
    ensure!(
        digest(source.guide_body, cancelled)? == profile.source_guide_sha256
            && digest(source.source_plan, cancelled)? == profile.source_plan_sha256,
        "K1 original guide or plan body differs"
    );
    ensure!(
        source.raw_components.len() <= 1024 && source.canonical_components.len() <= 1024,
        "K1 source inventory exceeds bound"
    );
    for owner in &profile.source_components {
        check(cancelled)?;
        let loaded =
            Guide::parse_preserving_selection(&serde_json::to_string(&owner.raw)?, "json")?;
        ensure!(
            loaded.components.len() == 1
                && serde_json::to_value(&loaded.components[0])? == owner.canonical,
            "K1 raw/canonical loader replay differs"
        );
        ensure!(
            source.raw_components.get(owner.original_index) == Some(&owner.raw)
                && serde_json::to_value(
                    source
                        .canonical_components
                        .get(owner.original_index)
                        .context("Missing K1 original canonical owner")?
                )? == owner.canonical,
            "K1 complete original owner context differs"
        );
    }
    ensure!(
        inputs.len() == profile.archives.len(),
        "K1 archive role inventory differs"
    );
    let mut members = BTreeMap::new();
    let mut roles = BTreeSet::new();
    let mut total = 0usize;
    for input in inputs {
        check(cancelled)?;
        ensure!(roles.insert(input.role), "Duplicate K1 archive role");
        let pin = profile
            .archives
            .iter()
            .find(|p| p.role == input.role)
            .context("Unknown K1 archive role")?;
        safe(input.filename)?;
        ensure!(
            input.filename == pin.filename && !input.filename.contains('/'),
            "K1 archive filename differs"
        );
        verify(input.bytes, pin.bytes, &pin.sha256, cancelled)?;
        let expected: Vec<_> = profile
            .assets
            .iter()
            .filter(|a| a.archive == input.role)
            .collect();
        ensure!(
            input.members.len() == expected.len() && input.members.len() <= 90,
            "K1 selected member inventory differs"
        );
        let mut names = BTreeSet::new();
        for member in input.members {
            safe(member.path)?;
            ensure!(
                names.insert(member.path.to_ascii_lowercase()),
                "K1 member path/case collision"
            );
            let asset = expected
                .iter()
                .find(|p| p.path == member.path)
                .context("Unknown K1 selected member/resolution")?;
            verify(member.bytes, asset.bytes, &asset.sha256, cancelled)?;
            total = total
                .checked_add(member.bytes.len())
                .context("K1 member byte overflow")?;
            ensure!(total <= LIMIT, "K1 total selected bytes exceed bound");
            ensure!(
                members
                    .insert(
                        (input.role.to_owned(), member.path.to_owned()),
                        member.bytes.to_vec()
                    )
                    .is_none(),
                "Duplicate K1 member"
            );
        }
    }
    check(cancelled)?;
    Ok(CandidateContract { profile, members })
}

fn edit_bytes(hex: &str) -> Result<Vec<u8>> {
    ensure!(
        matches!(hex.len(), 4 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid compiled K1 edit width"
    );
    (0..hex.len())
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&hex[index..index + 2], 16)?))
        .collect()
}
/// Validate every expected word and disjoint range before constructing output.
fn transform(
    input: &[u8],
    edits: &[Edit],
    input_sha: &str,
    output_sha: &str,
    cancelled: &AtomicBool,
) -> Result<(Vec<u8>, Value)> {
    verify(input, EXE_SIZE, input_sha, cancelled)?;
    let mut allowed = BTreeSet::new();
    for edit in edits {
        check(cancelled)?;
        let expected = edit_bytes(&edit.expected)?;
        let replacement = edit_bytes(&edit.replacement)?;
        ensure!(expected.len() == replacement.len(), "K1 edit changes width");
        let end = edit
            .offset
            .checked_add(expected.len())
            .context("K1 edit offset overflow")?;
        ensure!(
            input.get(edit.offset..end) == Some(expected.as_slice()),
            "K1 expected word differs"
        );
        for offset in edit.offset..end {
            ensure!(allowed.insert(offset), "Overlapping K1 edits");
        }
    }
    let mut output = input.to_vec();
    for edit in edits {
        check(cancelled)?;
        let replacement = edit_bytes(&edit.replacement)?;
        output[edit.offset..edit.offset + replacement.len()].copy_from_slice(&replacement);
    }
    verify(&output, EXE_SIZE, output_sha, cancelled)?;
    let mut changed = Vec::new();
    for (offset, (old, new)) in input.iter().zip(&output).enumerate() {
        if offset % 65536 == 0 {
            check(cancelled)?;
        }
        if old != new {
            ensure!(
                allowed.contains(&offset),
                "K1 unexpected output byte difference"
            );
            changed.push(json!({"Offset":offset,"Old":old,"New":new}));
        }
    }
    Ok((
        output,
        json!({"InputSha256":input_sha,"OutputSha256":output_sha,"Bytes":EXE_SIZE,"ExpectedWordEdits":edits,"ChangedBytes":changed,"EveryOtherByteUnchanged":true}),
    ))
}

impl CandidateContract {
    fn member(&self, role: &str, path: &str) -> Result<&[u8]> {
        self.members
            .get(&(role.to_owned(), path.to_owned()))
            .map(Vec::as_slice)
            .context("Missing admitted K1 member")
    }
    fn receipt(
        &self,
        role: &str,
        proof: Value,
        files: &BTreeMap<String, Vec<u8>>,
        cancelled: &AtomicBool,
    ) -> Result<Value> {
        let mut outputs = Vec::new();
        for (path, bytes) in files {
            outputs
                .push(json!({"Path":path,"Bytes":bytes.len(),"Sha256":digest(bytes,cancelled)?}));
        }
        Ok(
            json!({"Schema":1,"Kind":"K1WidescreenCandidateMaterializationOnly","Role":role,
            "ProfileSha256":DATA_SHA,"Profile":&self.profile,"VendorOutputEquivalenceProven":true,
            "VendorOutputEquivalenceScope":&self.profile.vendor_output_equivalence_scope,
            "UniwsVendorOutputEquivalenceProven":self.profile.uniws_vendor_output_equivalence_proven,
            "UniwsVendorWitness":&self.profile.uniws_vendor_witness,
            "FullComponentApproved":false,"InstalledSuccess":false,"ConsumerGaps":GAPS,
            "ByteProof":proof,"Outputs":outputs}),
        )
    }
    /// Minimal, deliberately created INI only for the authenticated absent-file
    /// profile. Some(empty) is still an existing input and must never be replaced.
    fn ini_from_authenticated_absence(
        &self,
        baseline_ini: Option<&[u8]>,
        cancelled: &AtomicBool,
    ) -> Result<(Vec<u8>, Value)> {
        check(cancelled)?;
        ensure!(
            baseline_ini.is_none(),
            "K1 INI creation rejects every existing input"
        );
        let policy = &self.profile.ini_creation;
        let expected = b"[Graphics Options]\r\nWidth=1920\r\nHeight=1080\r\n\r\n[Display Options]\r\nWidth=1920\r\nHeight=1080\r\n";
        ensure!(
            self.profile.profile == PROFILE_ID
                && policy.path == "swkotor.ini"
                && !policy.baseline_present
                && policy.baseline_engine_sha == "ca5520d829fdda55385f2c01974a23f83e71b032"
                && policy.policy == "create-only-on-explicit-None-reject-every-existing-input"
                && policy.encoding == "ASCII"
                && policy.line_endings == "CRLF"
                && !policy.game_launch_proven
                && policy.content.as_bytes() == expected
                && policy.bytes <= 1024,
            "K1 fixed absent-INI source profile differs"
        );
        verify(
            policy.content.as_bytes(),
            policy.bytes,
            &policy.sha256,
            cancelled,
        )?;
        let proof = json!({"Path":policy.path,"InputState":"Absent","ExplicitOptionInput":"None",
            "AuthenticatedBaselineEngineSha":policy.baseline_engine_sha,
            "AbsenceEvidenceSha256":policy.absence_evidence_sha256,
            "Operation":"DeliberateMinimalCreate","ExistingInputPolicy":"RejectEverySomeIncludingEmpty",
            "OutputBytes":policy.bytes,"OutputSha256":policy.sha256,"Encoding":policy.encoding,
            "LineEndings":policy.line_endings,"Sections":["Graphics Options","Display Options"],
            "Width":1920,"Height":1080,"GameLaunchProven":false});
        Ok((policy.content.as_bytes().to_vec(), proof))
    }
    /// Protected original is a replacement precondition, never a patch input.
    /// baseline_ini must explicitly attest absence; any existing bytes fail.
    pub(crate) fn hrmenus(
        &self,
        protected_baseline: &[u8],
        baseline_ini: Option<&[u8]>,
        cancelled: &AtomicBool,
    ) -> Result<CandidateOutput> {
        let (ini, ini_proof) = self.ini_from_authenticated_absence(baseline_ini, cancelled)?;
        verify(protected_baseline, 4395008, BASELINE, cancelled)?;
        let editable = self.member("editable", "KOTOR Editable Executable/swkotor.exe")?;
        let (uniws, uniws_proof) = transform(
            editable,
            &self.profile.uniws_edits,
            EDITABLE,
            UNIWS,
            cancelled,
        )?;
        let (executable, hrmenus_proof) = transform(
            &uniws,
            &self.profile.hrmenus_edits,
            UNIWS,
            HRMENUS,
            cancelled,
        )?;
        ensure!(
            executable.get(0x355788..0x35578c) == Some(&[0xb9, 0x6d, 0xdb, 0x3e][..]),
            "K1 letterbox-off bytes differ"
        );
        let mut files = BTreeMap::new();
        files.insert("swkotor.exe".into(), executable);
        for asset in &self.profile.assets {
            if asset.archive == "hrmenus" && asset.path.starts_with(GUI_PREFIX) {
                let name = asset
                    .path
                    .strip_prefix(GUI_PREFIX)
                    .context("K1 GUI prefix differs")?;
                ensure!(
                    !name.contains('/') && name.ends_with(".gui"),
                    "K1 GUI destination differs"
                );
                check(cancelled)?;
                ensure!(
                    files
                        .insert(
                            format!("Override/{name}"),
                            self.member("hrmenus", &asset.path)?.to_vec()
                        )
                        .is_none(),
                    "Duplicate K1 output path"
                );
            }
        }
        ensure!(files.len() == 82, "K1 requires exactly 81 GUI overlays");
        files.insert("swkotor.ini".into(), ini);
        let receipt = self.receipt("hrmenus", json!({"ProtectedBaselineSha256":BASELINE,"ProtectedBaselinePatched":false,"Uniws":uniws_proof,"Hrmenus":hrmenus_proof,"Ini":ini_proof}), &files, cancelled)?;
        check(cancelled)?;
        Ok(CandidateOutput { files, receipt })
    }
    /// Only the exact HRMenus stage can reach this native PE semantic operation.
    pub(crate) fn fourgb(
        &self,
        hrmenus_stage: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<CandidateOutput> {
        verify(hrmenus_stage, EXE_SIZE, HRMENUS, cancelled)?;
        ensure!(
            hrmenus_stage.get(..2) == Some(&b"MZ"[..])
                && hrmenus_stage.get(0x3c..0x40) == Some(&[0x10, 0x09, 0, 0][..])
                && hrmenus_stage.get(0x910..0x914) == Some(&b"PE\0\0"[..])
                && hrmenus_stage.get(0x914..0x916) == Some(&[0x4c, 0x01][..])
                && hrmenus_stage.get(0x928..0x92a) == Some(&[0x0b, 0x01][..]),
            "K1 PE32 x86 structure differs"
        );
        let edit = Edit {
            offset: 0x926,
            expected: "0f01".into(),
            replacement: "2f01".into(),
        };
        // The actual original tool also rewrites the PE checksum. A LAA-only
        // candidate was disproved by the isolated original-program observation.
        let checksum = Edit {
            offset: 0x968,
            expected: "f7733e00".into(),
            replacement: "11533e00".into(),
        };
        let (executable, proof) =
            transform(hrmenus_stage, &[edit, checksum], HRMENUS, FOURGB, cancelled)?;
        let files = BTreeMap::from([("swkotor.exe".into(), executable)]);
        let receipt = self.receipt("fourgb", proof, &files, cancelled)?;
        check(cancelled)?;
        Ok(CandidateOutput { files, receipt })
    }
    // The engine's whole-Galaxy operation calls this only after the authenticated
    // Vanilla namespace base completes its strict changed-game outcome contract.
    // These bytes alone never admit the whole Galaxy GUID.
    pub(crate) fn galaxy_overlay(&self, cancelled: &AtomicBool) -> Result<CandidateOutput> {
        check(cancelled)?;
        let path = format!("{GUI_PREFIX}galaxymap.gui");
        let files = BTreeMap::from([(
            "Override/galaxymap.gui".into(),
            self.member("galaxy", &path)?.to_vec(),
        )]);
        let receipt = self.receipt(
            "galaxy-overlay-after-strict-whole-base",
            Value::Null,
            &files,
            cancelled,
        )?;
        Ok(CandidateOutput { files, receipt })
    }
}

/// Read-only compiled input specification; callers cannot mint authority from it.
pub(crate) fn input_spec(cancelled: &AtomicBool) -> Result<Value> {
    verify(DATA.as_bytes(), DATA.len() as u64, DATA_SHA, cancelled)?;
    Ok(serde_json::from_str(DATA)?)
}
