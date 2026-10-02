//! Bounded pending links preserve each transport's arrival order.
use super::App;
use eframe::egui;

const PENDING_LIMIT: usize = 64;
const FRAME_LIMIT: usize = 64;

#[cfg(target_os = "macos")]
#[derive(Default)]
pub(super) struct State {
    #[cfg(target_os = "macos")]
    prefer_native: bool,
}

impl App {
    fn protocol_room(&self) -> bool {
        self.pending_protocol
            .len()
            .saturating_add(self.pending_nxm.len())
            < PENDING_LIMIT
    }

    pub(super) fn accept_protocol(&mut self, url: String) {
        if self.sharing.closing {
            return;
        }
        if modsync::single_instance::validate_protocol_request(&url).is_err() {
            self.logs.push("Protocol link rejected.".into());
            return;
        }
        if !self.protocol_room() {
            self.logs
                .push("Pending protocol queue is full; link was not retained.".into());
            return;
        }
        match modsync::protocol::parse(&url) {
            Ok(modsync::protocol::Link::Modsync { .. }) => self.pending_protocol.push_back(url),
            Ok(modsync::protocol::Link::Nxm { .. }) => {
                self.pending_nxm.push_back(url);
                self.start_pending_nxm_if_ready();
            }
            Err(_) => self.logs.push("Protocol link rejected.".into()),
        }
    }

    pub(super) fn poll_protocol_inbox(&mut self, ctx: &egui::Context) {
        for _ in 0..FRAME_LIMIT {
            // Leave acknowledged requests in their bounded transport when full.
            // Activation cannot leapfrog them and consumes the same frame budget.
            if self.sharing.closing || !self.protocol_room() {
                break;
            }
            #[cfg(not(target_os = "macos"))]
            let next = self
                .protocol_receiver
                .as_ref()
                .and_then(|rx| rx.try_recv().ok());
            #[cfg(target_os = "macos")]
            let next = {
                let (first, second) = if self.protocol_inbox.prefer_native {
                    (
                        self.native_protocol_receiver.as_ref(),
                        self.protocol_receiver.as_ref(),
                    )
                } else {
                    (
                        self.protocol_receiver.as_ref(),
                        self.native_protocol_receiver.as_ref(),
                    )
                };
                first
                    .and_then(|rx| rx.try_recv().ok())
                    .or_else(|| second.and_then(|rx| rx.try_recv().ok()))
            };
            let Some(url) = next else {
                break;
            };
            #[cfg(target_os = "macos")]
            {
                self.protocol_inbox.prefer_native = !self.protocol_inbox.prefer_native;
            }
            if url == modsync::single_instance::ACTIVATE_MESSAGE {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else {
                self.accept_protocol(url);
            }
        }
    }
}
