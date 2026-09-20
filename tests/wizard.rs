use modsync::{
    Guide,
    wizard::{Gate, InstallPhase, Page, Wizard, metadata_text},
};
use serde_json::json;

const OPEN: Gate = Gate {
    can_navigate_forward: true,
    validation_passed: true,
};

fn guide(game: &str, preamble: &str, aspyr: &str, widescreen: bool, warning: &str) -> Guide {
    serde_json::from_value(json!({
        "metadata": {"targetGame": game, "preambleContent": preamble,
            "aspyrExclusiveWarningContent": aspyr, "widescreenWarningContent": warning},
        "thisMod": [{"Guid": "base", "Name": "Base"},
            {"Guid": "wide", "Name": "Wide", "WidescreenOnly": widescreen}]
    }))
    .unwrap()
}

fn advance_to(wizard: &mut Wizard, page: Page) {
    while wizard.current() != page {
        assert!(wizard.next(OPEN));
    }
}

#[test]
fn exact_base_sequence_titles_and_metadata_predicates() {
    let expected = [
        "Welcome",
        "Mod Workspace Directory",
        "KOTOR Game Directory",
        "Mod Selection",
        "Download Process",
        "Validation",
        "Ready to Install",
        "Installing Mods",
        "Base Installation Complete",
        "Installation Complete",
    ];
    assert_eq!(
        Wizard::new(&Guide::default())
            .pages()
            .iter()
            .map(|p| p.title())
            .collect::<Vec<_>>(),
        expected
    );
    for game in ["K1", "KOTOR1", "KOTOR2", "TSL", "kotor2", "tsl", ""] {
        for content in ["", " \n\t", "Read this"] {
            let wizard = Wizard::new(&guide(game, content, content, true, content));
            assert_eq!(
                wizard.pages().contains(&Page::Preamble),
                !content.trim().is_empty()
            );
            assert_eq!(
                wizard.pages().contains(&Page::AspyrNotice),
                matches!(game, "KOTOR2" | "TSL") && !content.trim().is_empty()
            );
            assert!(!wizard.pages().contains(&Page::WidescreenModSelection));
            if wizard.pages().contains(&Page::AspyrNotice) {
                let index = wizard
                    .pages()
                    .iter()
                    .position(|p| *p == Page::AspyrNotice)
                    .unwrap();
                assert_eq!(wizard.pages()[index - 1], Page::GameDirectory);
                assert_eq!(wizard.pages()[index + 1], Page::ModSelection);
            }
        }
    }
    let guide: Guide = serde_json::from_value(json!({"thisMod": [],
        "metadata": {"TargetGame":"TSL", "AspyrExclusiveWarningContent":"Notice"},
        "preambleContent": "Markdown preamble"}))
    .unwrap();
    assert!(Wizard::new(&guide).pages().contains(&Page::AspyrNotice));
    assert_eq!(
        metadata_text(&guide, "preambleContent", "PreambleContent"),
        Some("Markdown preamble")
    );
}

#[test]
fn widescreen_insertion_requires_leaving_base_and_never_duplicates() {
    for wide in [false, true] {
        for warning in ["", " \t\n", "Read widescreen warning"] {
            let mut wizard = Wizard::new(&guide("KOTOR1", "", "", wide, warning));
            advance_to(&mut wizard, Page::Installing);
            assert_eq!(wizard.current().install_phase(), Some(InstallPhase::Base));
            assert!(!wizard.next(Gate::default()));
            assert!(wizard.next(OPEN));
            assert_eq!(wizard.current(), Page::BaseInstallComplete);
            assert!(!wizard.pages().contains(&Page::WidescreenModSelection));
            assert!(!wizard.next(Gate {
                validation_passed: false,
                ..OPEN
            }));
            assert!(wizard.next(OPEN));
            if !wide {
                assert_eq!(wizard.current(), Page::Finished);
                continue;
            }
            let mut tail = vec![];
            if !warning.trim().is_empty() {
                tail.push(Page::WidescreenNotice);
            }
            tail.extend([
                Page::WidescreenModSelection,
                Page::WidescreenInstalling,
                Page::WidescreenComplete,
                Page::Finished,
            ]);
            assert_eq!(&wizard.pages()[wizard.current_index()..], tail);
            let count = wizard.pages().len();
            assert!(!wizard.back());
            advance_to(&mut wizard, Page::WidescreenInstalling);
            assert_eq!(
                wizard.current().install_phase(),
                Some(InstallPhase::Remaining)
            );
            assert!(!wizard.next(Gate::default()));
            assert!(wizard.next(OPEN));
            assert_eq!(wizard.current(), Page::WidescreenComplete);
            assert!(wizard.next(OPEN));
            assert_eq!(wizard.current(), Page::Finished);
            assert!(!wizard.next(OPEN));
            assert_eq!(wizard.pages().len(), count);
            assert_eq!(
                wizard
                    .pages()
                    .iter()
                    .filter(|p| **p == Page::WidescreenModSelection)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn navigation_gates_bounds_history_and_new_guide_reset() {
    let mut wizard = Wizard::new(&guide("TSL", "Preamble", "Aspyr", true, "Wide"));
    assert!(!wizard.back());
    assert!(!wizard.next(Gate {
        can_navigate_forward: false,
        ..OPEN
    }));
    assert_eq!(wizard.current(), Page::Welcome);
    assert!(wizard.next(OPEN));
    assert!(wizard.next(OPEN));
    assert!(wizard.back());
    assert_eq!(wizard.current_index(), 1);
    assert_eq!(wizard.current(), Page::Preamble);
    advance_to(&mut wizard, Page::Finished);
    assert!(!wizard.back());
    wizard.reset(&Guide::default());
    assert_eq!(wizard.current_index(), 0);
    assert_eq!(wizard.current(), Page::Welcome);
    assert_eq!(wizard.pages().len(), 10);
    assert!(!wizard.pages().contains(&Page::Preamble));
    assert!(!wizard.pages().contains(&Page::AspyrNotice));
    assert!(!wizard.pages().contains(&Page::WidescreenModSelection));
}
