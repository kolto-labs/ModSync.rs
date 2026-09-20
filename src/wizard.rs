//! Presentation routes from the active C# WizardHostControl. Gates belong to the caller;
//! this module never validates paths, selects components, or starts installations.
use crate::Guide;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Welcome,
    Preamble,
    ModDirectory,
    GameDirectory,
    AspyrNotice,
    ModSelection,
    DownloadsExplain,
    Validate,
    InstallStart,
    Installing,
    BaseInstallComplete,
    WidescreenNotice,
    WidescreenModSelection,
    WidescreenInstalling,
    WidescreenComplete,
    Finished,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::Preamble => "Before You Begin",
            Self::ModDirectory => "Mod Workspace Directory",
            Self::GameDirectory => "KOTOR Game Directory",
            Self::AspyrNotice => "Aspyr Version Notice",
            Self::ModSelection => "Mod Selection",
            Self::DownloadsExplain => "Download Process",
            Self::Validate => "Validation",
            Self::InstallStart => "Ready to Install",
            Self::Installing => "Installing Mods",
            Self::BaseInstallComplete => "Base Installation Complete",
            Self::WidescreenNotice => "Widescreen Support",
            Self::WidescreenModSelection => "Widescreen Mod Selection",
            Self::WidescreenInstalling => "Installing Widescreen Mods",
            Self::WidescreenComplete => "Widescreen Installation Complete",
            Self::Finished => "Installation Complete",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome to the ModSync Installation Wizard",
            Self::Preamble => "Important information before starting the installation",
            Self::ModDirectory => "Choose where mod archives are downloaded and processed",
            Self::GameDirectory => "Point to your KOTOR installation folder",
            Self::AspyrNotice => "Important information about Aspyr-specific mods",
            Self::ModSelection => "Choose the mods you want to install",
            Self::DownloadsExplain => "Downloading required mod files",
            Self::Validate => "Validating your mod selection and installation environment",
            Self::InstallStart => "Review your selections and begin installation",
            Self::Installing => "Please wait while mods are being installed...",
            Self::BaseInstallComplete => "The base mod installation has finished successfully",
            Self::WidescreenNotice => "Information about widescreen mod installation",
            Self::WidescreenModSelection => "Select widescreen mods to install",
            Self::WidescreenInstalling => "Please wait...",
            Self::WidescreenComplete => "All widescreen mods have been installed",
            Self::Finished => "Thank you for using ModSync!",
        }
    }

    pub fn can_navigate_back(self) -> bool {
        !matches!(
            self,
            Self::Welcome
                | Self::Installing
                | Self::BaseInstallComplete
                | Self::WidescreenNotice
                | Self::WidescreenModSelection
                | Self::WidescreenInstalling
                | Self::WidescreenComplete
                | Self::Finished
        )
    }

    pub fn install_phase(self) -> Option<InstallPhase> {
        match self {
            Self::Installing => Some(InstallPhase::Base),
            Self::WidescreenInstalling => Some(InstallPhase::Remaining),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallPhase {
    Base,
    Remaining,
}

/// Supply the active page's button gate and validation result from shared state.
/// Installation gates must reflect successful engine completion, not mere exit.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gate {
    pub can_navigate_forward: bool,
    pub validation_passed: bool,
}

/// Read preserved guide metadata in the forms used by TOML/YAML and Markdown.
pub fn metadata_text<'a>(guide: &'a Guide, camel: &str, pascal: &str) -> Option<&'a str> {
    guide
        .metadata
        .get("metadata")
        .and_then(|m| m.get(camel).or_else(|| m.get(pascal)))
        .or_else(|| {
            guide
                .metadata
                .get(camel)
                .or_else(|| guide.metadata.get(pascal))
        })
        .and_then(serde_json::Value::as_str)
}

#[derive(Clone, Debug)]
pub struct Wizard {
    pages: Vec<Page>,
    current: usize,
    has_widescreen: bool,
    widescreen_warning: bool,
    widescreen_inserted: bool,
}

impl Wizard {
    pub fn new(guide: &Guide) -> Self {
        use Page::*;
        let nonblank = |camel, pascal| {
            metadata_text(guide, camel, pascal).is_some_and(|s| !s.trim().is_empty())
        };
        let mut pages = vec![Welcome];
        if nonblank("preambleContent", "PreambleContent") {
            pages.push(Preamble);
        }
        pages.extend([ModDirectory, GameDirectory]);
        if matches!(
            metadata_text(guide, "targetGame", "TargetGame"),
            Some("KOTOR2" | "TSL")
        ) && nonblank(
            "aspyrExclusiveWarningContent",
            "AspyrExclusiveWarningContent",
        ) {
            pages.push(AspyrNotice);
        }
        pages.extend([
            ModSelection,
            DownloadsExplain,
            Validate,
            InstallStart,
            Installing,
            BaseInstallComplete,
            Finished,
        ]);
        Self {
            pages,
            current: 0,
            has_widescreen: guide.components.iter().any(|c| {
                c.metadata
                    .get("WidescreenOnly")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
            }),
            widescreen_warning: nonblank("widescreenWarningContent", "WidescreenWarningContent"),
            widescreen_inserted: false,
        }
    }

    /// Loading a guide invalidates all route history and dynamically added pages.
    pub fn reset(&mut self, guide: &Guide) {
        *self = Self::new(guide);
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
    pub fn current_index(&self) -> usize {
        self.current
    }
    pub fn current(&self) -> Page {
        self.pages[self.current]
    }

    pub fn can_next(&self, gate: Gate) -> bool {
        self.current + 1 < self.pages.len() && gate.can_navigate_forward && gate.validation_passed
    }

    pub fn next(&mut self, gate: Gate) -> bool {
        if !self.can_next(gate) {
            return false;
        }
        if self.current() == Page::BaseInstallComplete
            && self.has_widescreen
            && !self.widescreen_inserted
        {
            let mut extra = Vec::new();
            if self.widescreen_warning {
                extra.push(Page::WidescreenNotice);
            }
            extra.extend([
                Page::WidescreenModSelection,
                Page::WidescreenInstalling,
                Page::WidescreenComplete,
            ]);
            let end = self.pages.len() - 1;
            self.pages.splice(end..end, extra);
            self.widescreen_inserted = true;
        }
        self.current += 1;
        true
    }

    pub fn can_back(&self) -> bool {
        self.current > 0 && self.current().can_navigate_back()
    }

    pub fn back(&mut self) -> bool {
        if !self.can_back() {
            return false;
        }
        self.current -= 1;
        true
    }
}
