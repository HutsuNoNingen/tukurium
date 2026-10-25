use crate::{Address, Edit};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const REPLICATION_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationMode {
    Reliable,
    Latest,
    Local,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldAccess {
    Edit,
    Command,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldSchema {
    pub id: String,
    pub label: String,
    pub value_type: String,
    pub replication: ReplicationMode,
    pub persistent: bool,
    pub access: FieldAccess,
    #[serde(default)]
    pub script_write: bool,
    #[serde(default)]
    pub script_drive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentSchema {
    pub type_id: String,
    pub local_id: u32,
    pub version: u32,
    pub fields: Vec<FieldSchema>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplicatedField {
    pub id: String,
    pub revision: String,
    pub value: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComponentState {
    pub address: Address,
    pub type_id: String,
    pub version: u32,
    pub fields: Vec<ReplicatedField>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplicatedObject {
    pub address: Address,
    pub revision: String,
    pub components: Vec<ComponentState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplicatedWorld {
    pub format_version: u32,
    pub world_revision: String,
    pub objects: Vec<ReplicatedObject>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplicationCommit {
    pub format_version: u32,
    pub world_revision: String,
    pub upserts: Vec<ReplicatedObject>,
    pub removed: Vec<Address>,
}

/// Binds an editor's lifetime to a reference field, independently of any particular UI.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EditContext {
    pub source: Address,
    pub field_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EditDraft {
    pub part: String,
    pub text: String,
    pub caret: u32,
    pub selection: [u32; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EditSession {
    pub owner: String,
    pub client_id: String,
    pub context: Option<EditContext>,
    pub target: Address,
    pub field_id: String,
    pub draft: EditDraft,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditSessionRequest {
    pub command_id: String,
    pub client_id: String,
    pub context: Option<EditContext>,
    pub target: Address,
    pub field_id: String,
    pub action: String,
    #[serde(default)]
    pub draft: EditDraft,
}

impl Edit {
    pub fn object_address(&self) -> Address {
        Address {
            instance_id: self.target.instance_id.clone(),
            local_id: 1,
        }
    }
}
