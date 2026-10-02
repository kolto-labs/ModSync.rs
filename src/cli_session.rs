//! CLI log/exporter ownership. Every dispatch result, including requested exit
//! codes, passes through session completion before the process exits.
use modsync::{logging, profiles, telemetry};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(crate) struct Exit(pub i32);
impl std::fmt::Display for Exit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command requested exit status {}", self.0)
    }
}
impl std::error::Error for Exit {}

pub(crate) struct Session {
    started: Instant,
    finished: bool,
    pub case_insensitive_pathing: bool,
}
impl Session {
    pub fn start(settings: Option<&Path>) -> Self {
        let mut session = Self {
            started: Instant::now(),
            finished: false,
            case_insensitive_pathing: true,
        };
        let store = match profiles::Store::from_settings_directory(settings) {
            Ok(store) => store,
            Err(error) => {
                eprintln!("Warning: logging settings unavailable: {error:#}");
                return session;
            }
        };
        let report = store.desktop_settings();
        session.case_insensitive_pathing = report.settings.case_insensitive_pathing;
        let directory = match profiles::settings_directory(settings) {
            Ok(directory) => directory,
            Err(error) => {
                eprintln!("Warning: logging directory unavailable: {error:#}");
                return session;
            }
        };
        if let Err(error) = logging::init(
            &directory,
            logging::Options {
                debug_logging: report.settings.debug_logging,
                mirror_stderr: false,
                ..Default::default()
            },
        ) {
            eprintln!("Warning: persistent logging unavailable: {error:#}");
        }
        logging::install_panic_hook();
        for warning in report.warnings {
            logging::warning(warning);
        }
        match telemetry::Config::load_from_settings_directory(settings) {
            Ok(config) => {
                telemetry::global().configure(&config);
                telemetry::global().record_session_start(0, 0);
            }
            Err(_) => logging::warning(
                "Telemetry configuration could not be loaded; telemetry remains disabled",
            ),
        }
        session
    }

    pub fn finish(mut self, success: bool) {
        self.finish_inner(success);
    }

    fn finish_inner(&mut self, success: bool) {
        if self.finished {
            return;
        }
        self.finished = true;
        telemetry::global().record_session_end(self.started.elapsed(), success);
        let _ = telemetry::global().shutdown(true, Duration::from_secs(2));
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.finish_inner(false);
    }
}

pub(crate) fn settings_directory(matches: &clap::ArgMatches) -> Option<&Path> {
    matches
        .try_get_one::<std::path::PathBuf>("settings_dir")
        .ok()
        .flatten()
        .map(|p| p.as_path())
        .or_else(|| {
            matches
                .subcommand()
                .and_then(|(_, child)| settings_directory(child))
        })
}

/// Explicit transport probes must keep their existing no-stored-settings boundary.
pub(crate) fn transport_probe(matches: &clap::ArgMatches) -> bool {
    [
        "nexus_api_base",
        "deadlystream_base",
        "mega_api_base",
        "extra_ca_file",
    ]
    .iter()
    .any(|name| matches.try_contains_id(name).unwrap_or(false))
        || matches
            .subcommand()
            .is_some_and(|(_, child)| transport_probe(child))
}
