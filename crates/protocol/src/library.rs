use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemIdentity {
    pub creator_id: String,
    pub owner_id: String,
    #[serde(default)]
    pub source_version: Option<String>,
    #[serde(default)]
    pub permissions: ItemPermissions,
    #[serde(default)]
    pub editors: Vec<ItemEditor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemEditor {
    pub user_id: String,
    pub edited_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemPermissions {
    pub modify: bool,
    pub redistribute: bool,
}

impl Default for ItemPermissions {
    fn default() -> Self {
        Self {
            modify: true,
            redistribute: false,
        }
    }
}
