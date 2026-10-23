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
    #[serde(default)]
    pub clock: ScriptClock,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScriptSchema {
    pub name: String,
    pub fields: Vec<ScriptField>,
    pub has_update: bool,
    #[serde(default)]
    pub has_drive: bool,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScriptStatus {
    pub target: Address,
    pub script_id: String,
    pub schema: Option<ScriptSchema>,
    pub error: Option<String>,
}

/// Shared playback anchors; derived values are never included in world commits.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ScriptClock {
    pub anchor: f64,
    pub elapsed: f64,
}
impl ScriptClock {
    pub fn time(&self, now: f64, running: bool) -> f64 {
        self.elapsed
            + if running {
                (now - self.anchor).max(0.0)
            } else {
                0.0
            }
    }
}

/// Local Engine-to-renderer output. This is not part of the replicated world.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DriveFrame {
    pub world_time: f64,
    pub objects: Vec<DrivenObject>,
    pub statuses: Vec<ScriptStatus>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DrivenObject {
    pub target: Address,
    pub values: BTreeMap<String, Value>,
}
