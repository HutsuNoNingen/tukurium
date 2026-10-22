use crate::Address;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScriptComponent {
    pub id: String,
    pub revision: String,
    pub enabled: bool,
    pub source: String,
    pub schema: ScriptSchema,
    pub parameters: BTreeMap<String, Value>,
    #[serde(default)]
    pub state: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScriptSchema {
    pub name: String,
    pub fields: Vec<ScriptField>,
    pub has_update: bool,
    pub has_interact: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScriptField {
    pub id: String,
    pub label: String,
    pub value_type: String,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptCommand {
    pub command_id: String,
    pub target: Address,
    #[serde(flatten)]
    pub action: ScriptAction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ScriptAction {
    Attach {
        source: String,
    },
    Update {
        script_id: String,
        expected_revision: String,
        source: String,
        enabled: bool,
        parameters: BTreeMap<String, Value>,
    },
    Remove {
        script_id: String,
        expected_revision: String,
    },
    Interact,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptStatus {
    pub target: Address,
    pub script_id: String,
    pub schema: Option<ScriptSchema>,
    pub error: Option<String>,
}
