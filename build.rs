//! Reserve an 8 MiB main-thread stack for the `modsync` executable on Windows.
//!
//! Windows links executables with a 1 MiB main-thread stack, while Linux and macOS
//! give the main thread 8 MiB. Unoptimized builds do not share stack slots between
//! temporaries, so the clap-derived command definition alone needs more than 1 MiB
//! there (`Action::augment_subcommands` reserves about 1.3 MiB), and every command,
//! including `--help`, overflowed before it could run. The desktop event loop must
//! stay on the main thread, so the main thread itself gets the larger reservation.
//! Only address space is reserved; pages are committed as the stack grows.

const MAIN_STACK_BYTES: u64 = 8 * 1024 * 1024;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let argument = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        format!("/STACK:{MAIN_STACK_BYTES}")
    } else {
        format!("-Wl,--stack,{MAIN_STACK_BYTES}")
    };
    println!("cargo:rustc-link-arg-bin=modsync={argument}");
}
