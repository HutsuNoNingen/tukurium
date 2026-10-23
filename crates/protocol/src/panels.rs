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
