//! A small, validated visual language stored as ScriptComponent.source.
//! The JSON graph is the source of truth; generated Luau only runs in the existing sandbox.
use serde::Deserialize;
use serde_json::Value;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet};

pub const PREFIX: &str = "@tukurium-graph-v1\n";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Graph {
    name: String,
    nodes: Vec<Node>,
    links: Vec<Link>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    id: String,
    kind: String,
    x: f64,
    y: f64,
    #[serde(default)]
    value: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    from: String,
    out: String,
    to: String,
    input: String,
}

struct Spec {
    inputs: &'static [(&'static str, &'static str)],
    outputs: &'static [(&'static str, &'static str)],
}

fn spec(kind: &str) -> Option<Spec> {
    let s = match kind {
        "on_update" | "on_interact" | "on_drive" => Spec {
            inputs: &[],
            outputs: &[("flow", "flow")],
        },
        "branch" => Spec {
            inputs: &[("flow", "flow"), ("test", "bool")],
            outputs: &[("yes", "flow"), ("no", "flow")],
        },
        "sequence" => Spec {
            inputs: &[("flow", "flow")],
            outputs: &[("first", "flow"), ("second", "flow")],
        },
        "write_field" | "drive_field" => Spec {
            inputs: &[("flow", "flow"), ("value", "any")],
            outputs: &[("next", "flow")],
        },
        "set_state" => Spec {
            inputs: &[("flow", "flow"), ("value", "any")],
            outputs: &[("next", "flow")],
        },
        "capture_camera" => Spec {
            inputs: &[("flow", "flow")],
            outputs: &[("next", "flow")],
        },
        "number" | "public_number" | "delta" | "time" | "world_time" => Spec {
            inputs: &[],
            outputs: &[("value", "number")],
        },
        "bool" => Spec {
            inputs: &[],
            outputs: &[("value", "bool")],
        },
        "string" | "to_string" | "join" => match kind {
            "to_string" => Spec {
                inputs: &[("value", "any")],
                outputs: &[("value", "string")],
            },
            "join" => Spec {
                inputs: &[("a", "string"), ("b", "string")],
                outputs: &[("value", "string")],
            },
            _ => Spec {
                inputs: &[],
                outputs: &[("value", "string")],
            },
        },
        "vec3" => Spec {
            inputs: &[],
            outputs: &[("value", "vec3")],
        },
        "color" => Spec {
            inputs: &[],
            outputs: &[("value", "color")],
        },
        "read_field" | "get_state" => Spec {
            inputs: &[],
            outputs: &[("value", "any")],
        },
        "display" => Spec {
            inputs: &[("value", "any")],
            outputs: &[("value", "any")],
        },
        "add" | "subtract" | "multiply" | "divide" | "modulo" | "min" | "max" => Spec {
            inputs: &[("a", "number"), ("b", "number")],
            outputs: &[("value", "number")],
        },
        "less" | "greater" => Spec {
            inputs: &[("a", "number"), ("b", "number")],
            outputs: &[("value", "bool")],
        },
        "equal" => Spec {
            inputs: &[("a", "any"), ("b", "any")],
            outputs: &[("value", "bool")],
        },
        "and" | "or" => Spec {
            inputs: &[("a", "bool"), ("b", "bool")],
            outputs: &[("value", "bool")],
        },
        "not" => Spec {
            inputs: &[("a", "bool")],
            outputs: &[("value", "bool")],
        },
        "select" => Spec {
            inputs: &[("test", "bool"), ("yes", "any"), ("no", "any")],
            outputs: &[("value", "any")],
        },
        "rotation_y" => Spec {
            inputs: &[("degrees", "number"), ("base", "any")],
            outputs: &[("value", "any")],
        },
        _ => return None,
    };
    Some(s)
}

pub fn is_graph(source: &str) -> bool {
    source.starts_with(PREFIX)
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
}

fn lua_string(s: &str) -> String {
    let mut quoted = String::from("\"");
    for b in s.bytes() {
        quoted.push_str(&format!("\\{b:03}"));
    }
    quoted.push('"');
    quoted
}

fn numeric(value: &Value) -> Result<String, String> {
    let n = value.as_f64().ok_or("Expected a number")?;
    if !n.is_finite() || n.abs() > 1e9 {
        return Err("Number outside script bounds".into());
    }
    Ok(n.to_string())
}

fn literal(value: &Value, kind: &str) -> Result<String, String> {
    match kind {
        "number" | "public_number" => numeric(value),
        "bool" => value
            .as_bool()
            .map(|b| b.to_string())
            .ok_or("Expected a boolean".into()),
        "string" => value
            .as_str()
            .filter(|s| s.len() <= 512)
            .map(lua_string)
            .ok_or("Expected a short string".into()),
        "vec3" | "color" => {
            let n = if kind == "vec3" { 3 } else { 4 };
            let a = value
                .as_array()
                .filter(|a| a.len() == n)
                .ok_or("Expected a numeric vector")?;
            Ok(format!(
                "{{{}}}",
                a.iter()
                    .map(numeric)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            ))
        }
        _ => Err("Invalid literal kind".into()),
    }
}

struct Compiler<'a> {
    nodes: HashMap<&'a str, &'a Node>,
    incoming: HashMap<(&'a str, &'a str), (&'a str, &'a str)>,
    outgoing: HashMap<(&'a str, &'a str), Vec<&'a str>>,
    graph: &'a Graph,
    visits: Cell<usize>,
}

impl Compiler<'_> {
    fn visit(&self) -> Result<(), String> {
        let next = self.visits.get() + 1;
        if next > 512 {
            return Err("Graph expands to too many operations".into());
        }
        self.visits.set(next);
        Ok(())
    }

    fn input(&self, id: &str, port: &str, stack: &mut HashSet<String>) -> Result<String, String> {
        let (from, _) = self
            .incoming
            .get(&(id, port))
            .ok_or_else(|| format!("Missing input {port} on {id}"))?;
        self.value(from, stack)
    }

    fn value(&self, id: &str, stack: &mut HashSet<String>) -> Result<String, String> {
        self.visit()?;
        if !stack.insert(id.to_owned()) {
            return Err("Value cycle in graph".into());
        }
        let node = self.nodes[id];
        let expression = match node.kind.as_str() {
            "number" | "bool" | "string" | "vec3" | "color" => literal(&node.value, &node.kind)?,
            "public_number" => format!("ctx.self.{}.p_{}", self.graph.name, id),
            "delta" => "dt".into(),
            "time" => "ctx.time".into(),
            "world_time" => "ctx.world_time".into(),
            "read_field" => format!("ctx:read(ctx.self.{}.ref_{})", self.graph.name, id),
            "get_state" => format!("ctx:get_state({}, 0)", lua_string(state_key(node)?)),
            "display" => self.input(id, "value", stack)?,
            "add" | "subtract" | "multiply" | "divide" | "modulo" | "less" | "greater"
            | "equal" | "and" | "or" | "min" | "max" => {
                let a = self.input(id, "a", stack)?;
                let b = self.input(id, "b", stack)?;
                let op = match node.kind.as_str() {
                    "add" => "+",
                    "subtract" => "-",
                    "multiply" => "*",
                    "divide" => "/",
                    "modulo" => "%",
                    "less" => "<",
                    "greater" => ">",
                    "equal" => "==",
                    "and" => "and",
                    "or" => "or",
                    _ => "",
                };
                if node.kind == "min" || node.kind == "max" {
                    format!("math.{}({a}, {b})", node.kind)
                } else {
                    format!("({a} {op} {b})")
                }
            }
            "not" => format!("(not {})", self.input(id, "a", stack)?),
            "to_string" => format!("tostring({})", self.input(id, "value", stack)?),
            "join" => format!(
                "({} .. {})",
                self.input(id, "a", stack)?,
                self.input(id, "b", stack)?
            ),
            "select" => format!(
                "(if {} then {} else {})",
                self.input(id, "test", stack)?,
                self.input(id, "yes", stack)?,
                self.input(id, "no", stack)?
            ),
            "rotation_y" => format!(
                "ctx:rotation_y({}, {})",
                self.input(id, "degrees", stack)?,
                self.input(id, "base", stack)?
            ),
            _ => return Err(format!("{} is not a value node", node.kind)),
        };
        stack.remove(id);
        Ok(expression)
    }

    fn actions(
        &self,
        id: &str,
        output: &str,
        drive: bool,
        capture_allowed: bool,
        stack: &mut HashSet<String>,
    ) -> Result<String, String> {
        let mut code = String::new();
        for next in self.outgoing.get(&(id, output)).into_iter().flatten() {
            self.visit()?;
            if !stack.insert((*next).to_owned()) {
                return Err("Flow cycle in graph".into());
            }
            let node = self.nodes[next];
            match node.kind.as_str() {
                "branch" => {
                    let test = self.input(next, "test", &mut HashSet::new())?;
                    code.push_str(&format!(
                        "if {test} then\n{}else\n{}end\n",
                        self.actions(next, "yes", drive, capture_allowed, stack)?,
                        self.actions(next, "no", drive, capture_allowed, stack)?
                    ));
                }
                "sequence" => {
                    code.push_str(&self.actions(next, "first", drive, capture_allowed, stack)?);
                    code.push_str(&self.actions(next, "second", drive, capture_allowed, stack)?);
                }
                "write_field" | "drive_field" => {
                    if (node.kind == "drive_field") != drive {
                        return Err(
                            "Drive belongs under On Drive; Write belongs under On Update/Interact"
                                .into(),
                        );
                    }
                    let value = self.input(next, "value", &mut HashSet::new())?;
                    code.push_str(&format!(
                        "ctx:{}(ctx.self.{}.ref_{}, {value})\n",
                        node.kind, self.graph.name, next
                    ));
                    code.push_str(&self.actions(next, "next", drive, capture_allowed, stack)?);
                }
                "set_state" => {
                    if drive {
                        return Err("Set State cannot run under On Drive".into());
                    }
                    let value = self.input(next, "value", &mut HashSet::new())?;
                    code.push_str(&format!(
                        "ctx:set_state({}, {value})\n",
                        lua_string(state_key(node)?)
                    ));
                    code.push_str(&self.actions(next, "next", drive, capture_allowed, stack)?);
                }
                "capture_camera" => {
                    if !capture_allowed { return Err("Capture Camera cannot run under On Drive".into()); }
                    code.push_str("ctx:capture_camera(ctx.self)\n");
                    code.push_str(&self.actions(next, "next", drive, capture_allowed, stack)?);
                }
                _ => return Err(format!("{} is not a flow node", node.kind)),
            }
            stack.remove(*next);
        }
        Ok(code)
    }
}

fn state_key(node: &Node) -> Result<&str, String> {
    node.value
        .as_str()
        .filter(|s| identifier(s))
        .ok_or_else(|| format!("{} requires a state name", node.kind))
}

pub fn compile(source: &str) -> Result<String, String> {
    let graph: Graph = serde_json::from_str(&source[PREFIX.len()..])
        .map_err(|e| format!("Invalid node graph: {e}"))?;
    if !identifier(&graph.name)
        || ["Transform", "Renderer", "Collider", "State"].contains(&graph.name.as_str())
    {
        return Err("Invalid graph component name".into());
    }
    if graph.nodes.is_empty() || graph.nodes.len() > 64 || graph.links.len() > 128 {
        return Err("Graph supports 1–64 nodes and up to 128 links".into());
    }
    let mut nodes = HashMap::new();
    let mut event_kinds = HashSet::new();
    let mut fields = BTreeMap::new();
    for node in &graph.nodes {
        if !identifier(&node.id)
            || !node.x.is_finite()
            || !node.y.is_finite()
            || node.x.abs() > 100000.0
            || node.y.abs() > 100000.0
            || !nodes.insert(node.id.as_str(), node).is_none()
        {
            return Err("Invalid or duplicate graph node id/position".into());
        }
        spec(&node.kind).ok_or_else(|| format!("Unknown node: {}", node.kind))?;
        match node.kind.as_str() {
            "on_update" | "on_interact" | "on_drive" => {
                if !event_kinds.insert(node.kind.as_str()) {
                    return Err("Only one node of each event kind is allowed".into());
                }
            }
            "public_number" => {
                fields.insert(
                    format!("p_{}", node.id),
                    format!(
                        "{{type='number',default={}}}",
                        literal(&node.value, "number")?
                    ),
                );
            }
            "read_field" | "write_field" | "drive_field" => {
                fields.insert(format!("ref_{}", node.id), "{type='field_ref'}".into());
            }
            "number" | "bool" | "string" | "vec3" | "color" => {
                literal(&node.value, &node.kind)?;
            }
            "get_state" | "set_state" => {
                state_key(node)?;
            }
            _ => {}
        }
    }
    if event_kinds.is_empty() {
        return Err("Graph needs an On Update, On Interact or On Drive event".into());
    }
    if fields.len() > 16 {
        return Err("Graph supports at most 16 public fields".into());
    }
    let mut incoming = HashMap::new();
    let mut outgoing: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
    let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut indegree: HashMap<&str, usize> = nodes.keys().map(|id| (*id, 0)).collect();
    for link in &graph.links {
        let from = nodes
            .get(link.from.as_str())
            .ok_or("Link source does not exist")?;
        let to = nodes
            .get(link.to.as_str())
            .ok_or("Link target does not exist")?;
        let out = spec(&from.kind)
            .unwrap()
            .outputs
            .iter()
            .find(|(name, _)| *name == link.out)
            .ok_or("Invalid output port")?;
        let input = spec(&to.kind)
            .unwrap()
            .inputs
            .iter()
            .find(|(name, _)| *name == link.input)
            .ok_or("Invalid input port")?;
        if out.1 != input.1 && out.1 != "any" && input.1 != "any" {
            return Err(format!("Incompatible ports: {} → {}", out.1, input.1));
        }
        if incoming
            .insert(
                (link.to.as_str(), link.input.as_str()),
                (link.from.as_str(), link.out.as_str()),
            )
            .is_some()
        {
            return Err("An input can have only one connection".into());
        }
        if out.1 == "flow" {
            outgoing
                .entry((link.from.as_str(), link.out.as_str()))
                .or_default()
                .push(link.to.as_str());
        }
        successors
            .entry(link.from.as_str())
            .or_default()
            .push(link.to.as_str());
        *indegree.get_mut(link.to.as_str()).unwrap() += 1;
    }
    let mut ready: Vec<&str> = indegree
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        for to in successors.get(id).into_iter().flatten() {
            let count = indegree.get_mut(to).unwrap();
            *count -= 1;
            if *count == 0 {
                ready.push(to);
            }
        }
    }
    if visited != nodes.len() {
        return Err("Graph contains a cycle".into());
    }
    let compiler = Compiler {
        graph: &graph,
        nodes,
        incoming,
        outgoing,
        visits: Cell::new(0),
    };
    let mut code = format!(
        "return {{name={},fields={{{}}},",
        lua_string(&graph.name),
        fields
            .iter()
            .map(|(id, value)| format!("{id}={value}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    for (kind, handler, drive) in [
        ("on_update", "update", false),
        ("on_interact", "on_interact", false),
        ("on_drive", "drive", true),
    ] {
        if let Some(node) = graph.nodes.iter().find(|n| n.kind == kind) {
            let actions = compiler.actions(&node.id, "flow", drive, !drive, &mut HashSet::new())?;
            code.push_str(&format!("{handler}=function(ctx,dt)\n{actions}end,"));
        }
    }
    code.push('}');
    if code.len() > 65536 {
        return Err("Graph expands to too much code".into());
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, Program, parameters};
    use kokoverse_protocol::{ObjectView, ScriptClock, ScriptComponent};
    use serde_json::json;

    fn source(graph: Value) -> String {
        format!("{PREFIX}{graph}")
    }

    #[test]
    fn basic_nodes_compile_into_existing_component_schema() {
        let graph = json!({"name":"Counter","nodes":[
            {"id":"e","kind":"on_update","x":0,"y":0},
            {"id":"a","kind":"set_state","x":20,"y":0,"value":"elapsed"},
            {"id":"s","kind":"get_state","x":0,"y":80,"value":"elapsed"},
            {"id":"d","kind":"delta","x":0,"y":120},
            {"id":"sum","kind":"add","x":20,"y":80},
            {"id":"p","kind":"public_number","x":10,"y":150,"value":3}
        ],"links":[
            {"from":"e","out":"flow","to":"a","input":"flow"},
            {"from":"s","out":"value","to":"sum","input":"a"},
            {"from":"d","out":"value","to":"sum","input":"b"},
            {"from":"sum","out":"value","to":"a","input":"value"}
        ]});
        let program = Program::compile(&source(graph)).unwrap();
        assert_eq!(program.schema.name, "Counter");
        assert!(program.schema.has_update);
        assert_eq!(program.schema.fields[0].id, "p_p");
        assert_eq!(program.schema.fields[0].default, json!(3));
    }

    #[test]
    fn capture_camera_runs_in_shared_callbacks_only() {
        let mut graph = json!({"name":"Shutter","nodes":[
            {"id":"event","kind":"on_interact","x":0,"y":0},
            {"id":"capture","kind":"capture_camera","x":100,"y":0}
        ],"links":[
            {"from":"event","out":"flow","to":"capture","input":"flow"}
        ]});
        assert!(Program::compile(&source(graph.clone())).unwrap().schema.has_interact);
        graph["nodes"][0]["kind"] = json!("on_update");
        assert!(Program::compile(&source(graph.clone())).unwrap().schema.has_update);
        graph["nodes"][0]["kind"] = json!("on_drive");
        assert!(Program::compile(&source(graph)).err().unwrap().contains("On Drive"));
    }

    #[test]
    fn node_countdown_reaches_capture_only_after_deadline() {
        let graph = json!({"name":"Shutter","nodes":[
            {"id":"click","kind":"on_interact","x":0,"y":0},
            {"id":"tick","kind":"on_update","x":0,"y":300},
            {"id":"arm","kind":"set_state","x":300,"y":0,"value":"deadline"},
            {"id":"due","kind":"set_state","x":900,"y":300,"value":"deadline"},
            {"id":"capture","kind":"capture_camera","x":700,"y":300},
            {"id":"branch","kind":"branch","x":500,"y":300},
            {"id":"and","kind":"and","x":350,"y":430},
            {"id":"later","kind":"greater","x":200,"y":500},
            {"id":"armed","kind":"greater","x":200,"y":600},
            {"id":"deadline","kind":"get_state","x":0,"y":550,"value":"deadline"},
            {"id":"now","kind":"time","x":0,"y":450},
            {"id":"add","kind":"add","x":150,"y":100},
            {"id":"three","kind":"number","x":0,"y":100,"value":3},
            {"id":"zero","kind":"number","x":0,"y":700,"value":0}
        ],"links":[
            {"from":"click","out":"flow","to":"arm","input":"flow"},
            {"from":"now","out":"value","to":"add","input":"a"},
            {"from":"three","out":"value","to":"add","input":"b"},
            {"from":"add","out":"value","to":"arm","input":"value"},
            {"from":"tick","out":"flow","to":"branch","input":"flow"},
            {"from":"now","out":"value","to":"later","input":"a"},
            {"from":"deadline","out":"value","to":"later","input":"b"},
            {"from":"deadline","out":"value","to":"armed","input":"a"},
            {"from":"zero","out":"value","to":"armed","input":"b"},
            {"from":"later","out":"value","to":"and","input":"a"},
            {"from":"armed","out":"value","to":"and","input":"b"},
            {"from":"and","out":"value","to":"branch","input":"test"},
            {"from":"branch","out":"yes","to":"capture","input":"flow"},
            {"from":"capture","out":"next","to":"due","input":"flow"},
            {"from":"zero","out":"value","to":"due","input":"value"}
        ]});
        let graph_source = source(graph);
        let program = Program::compile(&graph_source).unwrap();
        let mut component = ScriptComponent {
            id: "1".into(), revision: "0".into(), enabled: true, source: graph_source,
            schema: program.schema.clone(), parameters: parameters(&program.schema, &Default::default()).unwrap(),
            state: Default::default(), clock: ScriptClock::default(),
        };
        let object: ObjectView = serde_json::from_value(json!({
            "address":{"instance_id":"00000000-0000-0000-0000-000000000001","local_id":1},
            "object_revision":"0","world_position":[0,0,0],"fields":[],"camera":true,"scripts":[component.clone()]
        })).unwrap();
        let armed = program.run_at(&object, &component, Event::Interact, 0.0, 0.0).unwrap();
        assert_eq!(armed.state["deadline"], json!(3));
        component.state = armed.state;
        assert!(!program.run_at(&object, &component, Event::Update, 0.1, 2.0).unwrap().camera_capture);
        let due = program.run_at(&object, &component, Event::Update, 0.1, 3.1).unwrap();
        assert!(due.camera_capture);
        assert_eq!(due.state["deadline"], json!(0));
    }

    #[test]
    fn graph_rejects_bad_links_cycles_and_drive_write_mix() {
        let graph = json!({"name":"Graph1","nodes":[
            {"id":"e","kind":"on_drive","x":0,"y":0},
            {"id":"w","kind":"write_field","x":20,"y":0},
            {"id":"n","kind":"number","x":0,"y":60,"value":1}
        ],"links":[
            {"from":"e","out":"flow","to":"w","input":"flow"},
            {"from":"n","out":"value","to":"w","input":"value"}
        ]});
        assert!(
            Program::compile(&source(graph.clone()))
                .err()
                .unwrap()
                .contains("Drive belongs")
        );
        let mut bad = graph;
        bad["nodes"][1]["kind"] = json!("drive_field");
        let drive = Program::compile(&source(bad.clone())).unwrap();
        assert!(drive.schema.has_drive);
        assert_eq!(drive.schema.fields[0].value_type, "field_ref");
        bad["links"][1]["out"] = json!("missing");
        assert!(
            Program::compile(&source(bad))
                .err()
                .unwrap()
                .contains("Invalid output")
        );
        let cycle = json!({"name":"Graph1","nodes":[
            {"id":"e","kind":"on_update","x":0,"y":0},
            {"id":"a","kind":"add","x":0,"y":0},
            {"id":"b","kind":"add","x":0,"y":0}
        ],"links":[
            {"from":"a","out":"value","to":"b","input":"a"},
            {"from":"b","out":"value","to":"a","input":"a"}
        ]});
        assert!(
            Program::compile(&source(cycle))
                .err()
                .unwrap()
                .contains("cycle")
        );
    }

    #[test]
    fn select_preserves_false_and_strings_cannot_inject_code() {
        let graph = json!({"name":"Graph1","nodes":[
            {"id":"e","kind":"on_interact","x":0,"y":0},
            {"id":"a","kind":"set_state","x":20,"y":0,"value":"result"},
            {"id":"s","kind":"select","x":10,"y":50},
            {"id":"t","kind":"bool","x":0,"y":80,"value":true},
            {"id":"f","kind":"bool","x":0,"y":100,"value":false},
            {"id":"v","kind":"string","x":0,"y":120,"value":"\"; error('unsafe') --"}
        ],"links":[
            {"from":"e","out":"flow","to":"a","input":"flow"},
            {"from":"s","out":"value","to":"a","input":"value"},
            {"from":"t","out":"value","to":"s","input":"test"},
            {"from":"f","out":"value","to":"s","input":"yes"},
            {"from":"v","out":"value","to":"s","input":"no"}
        ]});
        let graph_source = source(graph);
        let program = Program::compile(&graph_source).unwrap();
        let component = ScriptComponent {
            id: "1".into(),
            revision: "0".into(),
            enabled: true,
            source: graph_source,
            schema: program.schema.clone(),
            parameters: parameters(&program.schema, &Default::default()).unwrap(),
            state: Default::default(),
            clock: ScriptClock::default(),
        };
        let object: ObjectView = serde_json::from_value(json!({
            "address":{"instance_id":"00000000-0000-0000-0000-000000000001","local_id":1},
            "object_revision":"0","world_position":[0,0,0],"fields":[],"scripts":[component.clone()]
        }))
        .unwrap();
        let output = program
            .run(&object, &component, Event::Interact, 0.0)
            .unwrap();
        assert_eq!(output.state["result"], json!(false));
    }

    #[test]
    fn every_value_node_can_be_compiled_from_typed_ports() {
        for kind in [
            "number",
            "public_number",
            "bool",
            "string",
            "vec3",
            "color",
            "delta",
            "time",
            "world_time",
            "read_field",
            "get_state",
            "add",
            "subtract",
            "multiply",
            "divide",
            "modulo",
            "min",
            "max",
            "less",
            "greater",
            "equal",
            "and",
            "or",
            "not",
            "select",
            "to_string",
            "join",
            "rotation_y",
            "display",
        ] {
            let value = match kind {
                "number" | "public_number" => json!(1),
                "bool" => json!(true),
                "string" => json!("hello"),
                "vec3" => json!([0, 1, 2]),
                "color" => json!([1, 1, 1, 1]),
                "get_state" => json!("counter"),
                _ => Value::Null,
            };
            let mut nodes = vec![
                json!({"id":"e","kind":"on_update","x":0,"y":0}),
                json!({"id":"a","kind":"set_state","x":0,"y":0,"value":"result"}),
                json!({"id":"v","kind":kind,"x":0,"y":0,"value":value}),
            ];
            let mut links = vec![
                json!({"from":"e","out":"flow","to":"a","input":"flow"}),
                json!({"from":"v","out":"value","to":"a","input":"value"}),
            ];
            for (index, (port, ty)) in spec(kind).unwrap().inputs.iter().enumerate() {
                let (source_kind, default) = match *ty {
                    "bool" => ("bool", json!(true)),
                    "string" => ("string", json!("x")),
                    _ => ("number", json!(2)),
                };
                let id = format!("i{index}");
                nodes.push(json!({"id":id,"kind":source_kind,"x":0,"y":0,"value":default}));
                links.push(json!({"from":id,"out":"value","to":"v","input":port}));
            }
            let graph = json!({"name":"Graph1","nodes":nodes,"links":links});
            assert!(Program::compile(&source(graph)).is_ok(), "{kind} failed");
        }
    }
}
