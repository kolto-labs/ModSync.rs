use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Detected {
    #[default]
    Unknown,
    Kotor1,
    Kotor2Legacy,
    Kotor2Aspyr,
}

const KOTOR1: &[&str] = &[
    "streamwaves",
    "swkotor.exe",
    "swkotor.ini",
    "rims",
    "utils",
    "32370_install.vdf",
    "miles/mssds3d.m3d",
    "miles/msssoft.m3d",
    "data/party.bif",
    "data/player.bif",
    "modules/global.mod",
    "modules/legal.mod",
    "modules/mainmenu.mod",
];
const KOTOR2: &[&str] = &[
    "streamvoice",
    "swkotor2.exe",
    "swkotor2.ini",
    "LocalVault",
    "LocalVault/test.bic",
    "LocalVault/testold.bic",
    "miles/binkawin.asi",
    "miles/mssds3d.flt",
    "miles/mssdolby.flt",
    "miles/mssogg.asi",
    "data/Dialogs.bif",
];
const ASPYR: &[&str] = &[
    "override/cntrl_ps3_eng.tga",
    "override/cntrl_ps3_fre.tga",
    "override/cntrl_ps3_ger.tga",
    "override/cntrl_ps3_ita.tga",
    "override/cntrl_ps3_spa.tga",
    "override/cntrl_xb360_eng.tga",
    "override/cntrl_xb360_fre.tga",
    "override/cntrl_xb360_ger.tga",
    "override/cntrl_xb360_ita.tga",
    "override/cntrl_xb360_spa.tga",
    "override/cus_button_a.tga",
    "override/cus_button_aps.tga",
    "override/cus_button_b.tga",
    "override/cus_button_bps.tga",
    "override/cus_button_x.tga",
    "override/cus_button_xps.tga",
    "override/cus_button_y.tga",
    "override/cus_button_yps.tga",
    "override/cus_gpad_bg.tga",
    "override/cus_gpad_fper.tga",
    "override/cus_gpad_fper2.tga",
    "override/cus_gpad_gen.tga",
    "override/cus_gpad_gen2.tga",
    "override/cus_gpad_hand.tga",
    "override/cus_gpad_hand2.tga",
    "override/cus_gpad_help.tga",
    "override/cus_gpad_help2.tga",
    "override/cus_gpad_map.tga",
    "override/cus_gpad_map2.tga",
    "override/cus_gpad_save.tga",
    "override/cus_gpad_save2.tga",
    "override/cus_gpad_solo.tga",
    "override/cus_gpad_solo2.tga",
    "override/cus_gpad_solox.tga",
    "override/cus_gpad_solox2.tga",
    "override/cus_gpad_ste.tga",
    "override/cus_gpad_ste2.tga",
    "override/cus_gpad_ste3.tga",
    "override/custom.txt",
    "override/custpnl_p.gui",
    "override/d2xfnt_d16x16b.tga",
    "override/d2xfont16x16b_ps.tga",
    "override/d2xfont16x16b.tga",
    "override/d3xfnt_d16x16b.tga",
    "override/d3xfont16x16b_ps.tga",
    "override/d3xfont16x16b.tga",
    "override/diafnt16x16b_ps.tga",
    "override/dialogfont16x16b.tga",
    "override/equip_p.gui",
    "override/fx_step_splash.MDL",
    "override/gamepad.txt",
    "override/gui_scroll.wav",
    "override/handmaiden.DLG",
    "override/lbl_miscroll_op",
];

fn score(root: &Path, checks: &[&str]) -> usize {
    checks
        .iter()
        .filter(|relative| root.join(relative).exists())
        .count()
}

pub fn detect(root: &Path) -> Detected {
    if !root.is_dir() {
        return Detected::Unknown;
    }
    let kotor1 = score(root, KOTOR1);
    let kotor2 = score(root, KOTOR2);
    if kotor1 > kotor2 {
        return Detected::Kotor1;
    }
    if kotor2 == 0 {
        return Detected::Unknown;
    }
    if !root.join("override").is_dir() {
        return Detected::Kotor2Legacy;
    }
    if score(root, ASPYR) * 10 >= ASPYR.len() * 7 {
        Detected::Kotor2Aspyr
    } else {
        Detected::Kotor2Legacy
    }
}

pub fn resolve_install_root(root: &Path) -> PathBuf {
    let steamassets = root.join("steamassets");
    if steamassets.join("override").is_dir()
        && (steamassets.join("dialog.tlk").is_file() || steamassets.join("chitin.key").is_file())
    {
        steamassets
    } else {
        root.to_path_buf()
    }
}

pub fn aspyr_exclusive(component: &crate::Component) -> bool {
    component
        .metadata
        .get("AspyrExclusive")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

pub fn selectable(component: &crate::Component, game: Detected) -> bool {
    !aspyr_exclusive(component) || game == Detected::Kotor2Aspyr
}

pub fn enforce_selectable(components: &mut [crate::Component], game: Detected) {
    let blocked: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| component.is_selected && !selectable(component, game))
        .map(|(index, _)| index)
        .collect();
    for index in blocked {
        crate::selection::set_component_selected(components, index, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_reference_game_scores_and_resolves_aspyr_content_root() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(detect(root.path()), Detected::Unknown);
        std::fs::write(root.path().join("swkotor.exe"), b"").unwrap();
        assert_eq!(detect(root.path()), Detected::Kotor1);

        let k2 = tempfile::tempdir().unwrap();
        std::fs::write(k2.path().join("swkotor2.exe"), b"").unwrap();
        assert_eq!(detect(k2.path()), Detected::Kotor2Legacy);
        std::fs::create_dir(k2.path().join("override")).unwrap();
        for relative in ASPYR.iter().take((ASPYR.len() * 7).div_ceil(10)) {
            let path = k2.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }
        assert_eq!(detect(k2.path()), Detected::Kotor2Aspyr);

        let parent = tempfile::tempdir().unwrap();
        let assets = parent.path().join("steamassets");
        std::fs::create_dir_all(assets.join("override")).unwrap();
        std::fs::write(assets.join("chitin.key"), b"").unwrap();
        assert_eq!(resolve_install_root(parent.path()), assets);
        assert_eq!(resolve_install_root(k2.path()), k2.path());
    }

    #[test]
    fn aspyr_components_are_removed_from_non_aspyr_selections() {
        let mut component = crate::Component {
            is_selected: true,
            ..Default::default()
        };
        component
            .metadata
            .insert("AspyrExclusive".into(), serde_json::Value::Bool(true));
        let mut components = vec![component];

        enforce_selectable(&mut components, Detected::Kotor2Legacy);
        assert!(!components[0].is_selected);
        components[0].is_selected = true;
        enforce_selectable(&mut components, Detected::Kotor2Aspyr);
        assert!(components[0].is_selected);
    }
}
