use crate::connection::ConnCtx;
use crate::connection::participants::ParticipantsPanel;
use crate::connection::rpc::{RpcPanel, RpcUiState};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum RightTab {
    #[default]
    Participants,
    Rpc,
}

/// Persistent state for the right panel, owned by the window so it survives
/// across frames. `rpc` is public because the window's event pump delivers RPC
/// results and disconnects straight into it; `tab` is touched only by
/// [`RightPanel`], so it stays private.
#[derive(Default)]
pub struct RightPanelState {
    tab: RightTab,
    pub rpc: RpcUiState,
}

/// The right panel: a Participants / RPC tab selector and the active tab's view.
pub struct RightPanel<'a> {
    pub ctx: &'a ConnCtx<'a>,
    pub state: &'a mut RightPanelState,
}

impl egui::Widget for RightPanel<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let RightPanel { ctx, state } = self;
        ui.scope(|ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.tab, RightTab::Participants, "Participants");
                ui.selectable_value(&mut state.tab, RightTab::Rpc, "RPC");
            });
            ui.separator();

            match state.tab {
                RightTab::Participants => {
                    ui.add(ParticipantsPanel { ctx });
                }
                RightTab::Rpc => {
                    ui.add(RpcPanel {
                        state: &mut state.rpc,
                        ctx,
                    });
                }
            }
        })
        .response
    }
}
