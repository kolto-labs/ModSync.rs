//! The saved DeadlyStream sign-in, kept in the system credential store (Windows
//! Credential Manager, the macOS Keychain or the Secret Service on Linux), as
//! the original app kept its sign-in in the system keyring. Only the signed-in
//! session cookies and the account name are kept, never the password; when the
//! session expires the player is asked to sign in again.
use serde::{Deserialize, Serialize};

const SERVICE: &str = "ModSync KOTOR Mod Installer";
const ENTRY: &str = "deadlystream-session";
/// Cookie names that carry a DeadlyStream (Invision Community) sign-in.
const ESSENTIAL: [&str; 5] = [
    "member_id",
    "login_key",
    "device_key",
    "session",
    "loggedin",
];

#[derive(Serialize, Deserialize)]
struct Saved {
    username: String,
    session: String,
}

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ENTRY)
}

/// The saved account name and session cookies, if any.
pub fn load() -> Option<(String, String)> {
    let text = entry().ok()?.get_password().ok()?;
    let saved: Saved = serde_json::from_str(&text).ok()?;
    (!saved.session.trim().is_empty()).then_some((saved.username, saved.session))
}

/// The session's sign-in cookies only, for stores that limit the size.
fn essential(session: &str) -> String {
    session
        .split(';')
        .map(str::trim)
        .filter(|pair| {
            let name = pair
                .split('=')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            ESSENTIAL.iter().any(|part| name.contains(part))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Save the sign-in. Falls back to the sign-in cookies alone when the store
/// refuses the full set (Windows limits a credential's size).
pub fn save(username: &str, session: &str) -> Result<(), String> {
    let store = |session: &str| {
        let text = serde_json::to_string(&Saved {
            username: username.to_owned(),
            session: session.to_owned(),
        })
        .map_err(|e| e.to_string())?;
        entry()
            .and_then(|e| e.set_password(&text))
            .map_err(|e| e.to_string())
    };
    store(session).or_else(|_| store(&essential(session)))
}

/// Forget the saved sign-in.
pub fn forget() {
    if let Ok(entry) = entry() {
        let _ = entry.delete_credential();
    }
}
