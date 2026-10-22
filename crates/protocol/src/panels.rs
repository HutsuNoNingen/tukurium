use crate::Address;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PanelAttachment {
    pub user_id: String,
    pub offset: [f64; 3],
    pub rotation: [f64; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorldPanel {
    pub size: [u32; 2],
    pub attachment: Option<PanelAttachment>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InspectorView {
    pub target: Option<Address>,
    pub scroll: [f64; 2],
}

/// A bounded, ephemeral edit lease. Never serialized in world saves.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PanelEditSession {
    pub owner: String,
    pub client_id: String,
    pub panel: Address,
    pub target: Address,
    pub field_id: String,
    pub draft: PanelDraft,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PanelDraft {
    pub part: String,
    pub text: String,
    pub caret: u32,
    pub selection: [u32; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PanelInput {
    pub command_id: String,
    pub client_id: String,
    pub panel: Address,
    pub target: Address,
    pub field_id: String,
    pub action: String,
    #[serde(default)]
    pub draft: PanelDraft,
}
