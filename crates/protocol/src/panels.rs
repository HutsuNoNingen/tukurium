use crate::Address;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypedReference {
    Slot { target: Address },
    Component { target: Address },
    Field { target: Address, field_id: String },
}
impl TypedReference {
    pub fn target(&self) -> &Address {
        match self {
            Self::Slot { target } | Self::Component { target } | Self::Field { target, .. } => {
                target
            }
        }
    }
}

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
    #[serde(default)]
    pub scripts_open: bool,
    #[serde(default)]
    pub script_id: Option<String>,
    #[serde(default)]
    pub tree: InspectorTree,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct InspectorTree {
    pub root: Option<Address>,
    pub collapsed: Vec<Address>,
}
