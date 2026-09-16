use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use modsync::{Guide, profiles::Store, recovery};
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone, Copy, ValueEnum)]
enum Action {
    List,
    Show,
    Create,
    Clone,
    Rename,
    Delete,
    Activate,
    Capture,
    Apply,
}
#[derive(Args)]
pub struct ProfileArgs {
    #[arg(long, value_enum, default_value = "list")]
    action: Action,
    #[arg(long)]
    settings_dir: Option<PathBuf>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    to: Option<String>,
    #[arg(long)]
    input: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    game_dir: Option<PathBuf>,
    #[arg(long)]
    source_dir: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}
impl ProfileArgs {
    pub fn run(self) -> Result<()> {
        let directory = modsync::profiles::settings_directory(self.settings_dir.as_deref())?;
        let store = Store::new(&directory)?;
        let required = |value: Option<&str>, flag: &str| -> Result<String> {
            let v = value
                .context(format!("Profile action requires --{flag}"))?
                .trim();
            ensure!(!v.is_empty(), "Profile action requires --{flag}");
            Ok(v.into())
        };
        let result = match self.action {
            Action::List => {
                let active = store.active_name()?;
                json!(store.list()?.iter().map(|p|json!({"Name":p.name,"KotorDirectory":p.kotor_directory,"ModDirectory":p.mod_directory,"InstructionFilePath":p.instruction_file_path,"ComponentSelectionCount":p.component_selections.len(),"CreatedUtc":p.created_utc,"LastUsedUtc":p.last_used_utc,"IsActive":active.as_ref().is_some_and(|s|s.eq_ignore_ascii_case(&p.name))})).collect::<Vec<_>>())
            }
            Action::Show => store
                .require(&required(self.name.as_deref(), "name")?)?
                .cli_value(),
            Action::Create => store
                .create(&required(self.name.as_deref(), "name")?)?
                .cli_value(),
            Action::Clone => store
                .clone_profile(
                    &required(self.from.as_deref(), "from")?,
                    &required(self.to.as_deref(), "to")?,
                )?
                .cli_value(),
            Action::Rename => store
                .rename(
                    &required(self.from.as_deref(), "from")?,
                    &required(self.to.as_deref(), "to")?,
                )?
                .cli_value(),
            Action::Delete => {
                let name = required(self.name.as_deref(), "name")?;
                ensure!(store.delete(&name)?, "Profile '{name}' was not found");
                json!({"Deleted":name})
            }
            Action::Activate => store
                .activate(&required(self.name.as_deref(), "name")?)?
                .cli_value(),
            Action::Capture => {
                let input = self.input.as_ref().context("Capture requires --input")?;
                let guide = Guide::load(input)?;
                store
                    .capture(
                        &required(self.name.as_deref(), "name")?,
                        &guide,
                        Some(input),
                        self.game_dir.as_deref(),
                        self.source_dir.as_deref(),
                    )?
                    .cli_value()
            }
            Action::Apply => {
                let input = self.input.as_ref().context("Apply requires --input")?;
                let output = self.output.as_ref().context("Apply requires --output")?;
                let name = required(self.name.as_deref(), "name")?;
                let mut guide = Guide::load(input)?;
                let profile = store.apply(&name, &mut guide)?;
                let format = output
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("toml");
                let text = guide.render(format)?;
                recovery::atomic_write(&std::path::absolute(output)?, text.as_bytes())?;
                profile.cli_value()
            }
        };
        // JSON is also the default output; --json matches the reference invocation.
        let _ = self.json;
        println!("{}", serde_json::to_string_pretty(&result)?);
        Ok(())
    }
}
