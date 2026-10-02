//! Main-thread macOS URL events. The GUI consumes the same parsed, bounded text
//! used by command-line and single-instance delivery.
use anyhow::{Result, ensure};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
use objc2_foundation::{
    MainThreadMarker, NSAppleEventDescriptor, NSAppleEventManager, NSObject, NSObjectProtocol,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};

const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
const ERROR_NUMBER: u32 = u32::from_be_bytes(*b"errn");
static INSTALLED: AtomicBool = AtomicBool::new(false);

struct State {
    sender: SyncSender<String>,
}

define_class!(
    // NSObject requires no additional subclass invariants. All methods and
    // ownership are confined to the AppKit main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = State]
    struct ModSyncUrlReceiver;

    unsafe impl NSObjectProtocol for ModSyncUrlReceiver {}

    impl ModSyncUrlReceiver {
        #[unsafe(method(handleModSyncEvent:withReplyEvent:))]
        fn handle(&self, event: &NSAppleEventDescriptor, reply: &NSAppleEventDescriptor) {
            let accepted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // The Apple Event direct-object keyword has the UInt32 ABI.
                let descriptor: Option<Retained<NSAppleEventDescriptor>> = unsafe {
                    msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT]
                };
                let Some(value) = descriptor.and_then(|value| value.stringValue()) else { return false; };
                if value.length() > 8192 { return false; }
                let text = value.to_string();
                if text.is_empty() || text.len() > 8192 || crate::protocol::parse(&text).is_err() { return false; }
                self.ivars().sender.try_send(text).is_ok()
            })).unwrap_or(false);
            let error = NSAppleEventDescriptor::descriptorWithInt32(if accepted { 0 } else { -1708 });
            // Acknowledgement means validated and queued, not downloaded/installed.
            unsafe { let _: () = msg_send![reply, setParamDescriptor: &*error, forKeyword: ERROR_NUMBER]; }
            if !accepted { crate::logging::warning("A macOS URL event was rejected or its queue was full"); }
        }
    }
);

/// Retain for the entire native event loop and drop on that same main thread.
pub struct Guard {
    manager: Retained<NSAppleEventManager>,
    _handler: Retained<ModSyncUrlReceiver>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![&*self.manager, removeEventHandlerForEventClass: GET_URL, andEventID: GET_URL];
        }
        INSTALLED.store(false, Ordering::Release);
    }
}

pub fn install() -> Result<(Receiver<String>, Guard)> {
    let thread = MainThreadMarker::new()
        .ok_or_else(|| anyhow::anyhow!("macOS URL registration requires the main thread"))?;
    ensure!(
        INSTALLED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok(),
        "macOS URL receiver is already installed"
    );
    let (sender, receiver) = mpsc::sync_channel(64);
    let allocated = ModSyncUrlReceiver::alloc(thread).set_ivars(State { sender });
    let handler: Retained<ModSyncUrlReceiver> = unsafe { msg_send![super(allocated), init] };
    let manager = NSAppleEventManager::sharedAppleEventManager();
    unsafe {
        let _: () = msg_send![&*manager, setEventHandler: &*handler,
            andSelector: sel!(handleModSyncEvent:withReplyEvent:),
            forEventClass: GET_URL, andEventID: GET_URL];
    }
    Ok((
        receiver,
        Guard {
            manager,
            _handler: handler,
        },
    ))
}
