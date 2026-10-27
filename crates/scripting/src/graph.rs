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
        "on_update" | "on_interact" | "on_button_press" | "on_button_release" | "on_contact_start" | "on_contact_end" | "on_drive" => Spec {
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
        "for_range" => Spec {
            inputs: &[("flow", "flow"), ("start", "number"), ("end", "number"), ("step", "number")],
            outputs: &[("body", "flow"), ("done", "flow"), ("index", "number")],
        },
        "while_loop" => Spec {
            inputs: &[("flow", "flow"), ("test", "bool")],
            outputs: &[("body", "flow"), ("done", "flow")],
        },
        "break_loop" | "continue_loop" => Spec {
            inputs: &[("flow", "flow")], outputs: &[],
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
        "contact_other_id" => Spec { inputs: &[], outputs: &[("value", "string")] },
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
        "ceil" | "abs" | "floor" | "sin" | "cos" | "negate" | "sqrt" | "round"
        | "sign" | "tan" | "asin" | "acos" | "log" | "exp" => Spec {
            inputs: &[("value", "number")],
            outputs: &[("value", "number")],
        },
        "power" => Spec {
            inputs: &[("a", "number"), ("b", "number")], outputs: &[("value", "number")],
        },
        "atan2" => Spec {
            inputs: &[("y", "number"), ("x", "number")], outputs: &[("value", "number")],
        },
        "remap" => Spec {
            inputs: &[("value", "number"), ("in_min", "number"), ("in_max", "number"), ("out_min", "number"), ("out_max", "number")],
            outputs: &[("value", "number")],
        },
        "smoothstep" => Spec {
            inputs: &[("edge0", "number"), ("edge1", "number"), ("value", "number")], outputs: &[("value", "number")],
        },
        "ping_pong" => Spec {
            inputs: &[("value", "number"), ("length", "number")], outputs: &[("value", "number")],
        },
        "less_equal" | "greater_equal" => Spec {
            inputs: &[("a", "number"), ("b", "number")], outputs: &[("value", "bool")],
        },
        "not_equal" => Spec {
            inputs: &[("a", "any"), ("b", "any")], outputs: &[("value", "bool")],
        },
        "xor" => Spec {
            inputs: &[("a", "bool"), ("b", "bool")], outputs: &[("value", "bool")],
        },
        "parse_number" => Spec { inputs: &[("text", "string")], outputs: &[("value", "number")] },
        "number_to_bool" => Spec { inputs: &[("value", "number")], outputs: &[("value", "bool")] },
        "bool_to_number" => Spec { inputs: &[("value", "bool")], outputs: &[("value", "number")] },
        "string_length" => Spec { inputs: &[("text", "string")], outputs: &[("value", "number")] },
        "string_contains" | "string_starts_with" | "string_ends_with" => Spec {
            inputs: &[("text", "string"), ("part", "string")], outputs: &[("value", "bool")],
        },
        "string_slice" => Spec {
            inputs: &[("text", "string"), ("start", "number"), ("count", "number")], outputs: &[("value", "string")],
        },
        "vec3_angle" => Spec {
            inputs: &[("a", "vec3"), ("b", "vec3")], outputs: &[("value", "number")],
        },
        "vec3_reflect" => Spec {
            inputs: &[("value", "vec3"), ("normal", "vec3")], outputs: &[("value", "vec3")],
        },
        "vec3_move_towards" => Spec {
            inputs: &[("a", "vec3"), ("b", "vec3"), ("max_delta", "number")], outputs: &[("value", "vec3")],
        },
        "color_make" => Spec {
            inputs: &[("r", "number"), ("g", "number"), ("b", "number"), ("a", "number")], outputs: &[("value", "color")],
        },
        "color_split" => Spec {
            inputs: &[("value", "color")], outputs: &[("r", "number"), ("g", "number"), ("b", "number"), ("a", "number")],
        },
        "color_lerp" => Spec {
            inputs: &[("a", "color"), ("b", "color"), ("t", "number")], outputs: &[("value", "color")],
        },
        "clamp" => Spec {
            inputs: &[("value", "number"), ("min", "number"), ("max", "number")],
            outputs: &[("value", "number")],
        },
        "lerp" => Spec {
            inputs: &[("a", "number"), ("b", "number"), ("t", "number")],
            outputs: &[("value", "number")],
        },
        "vec3_make" => Spec {
            inputs: &[("x", "number"), ("y", "number"), ("z", "number")],
            outputs: &[("value", "vec3")],
        },
        "vec3_split" => Spec {
            inputs: &[("value", "vec3")],
            outputs: &[("x", "number"), ("y", "number"), ("z", "number")],
        },
        "vec3_add" | "vec3_subtract" | "vec3_cross" => Spec {
            inputs: &[("a", "vec3"), ("b", "vec3")],
            outputs: &[("value", "vec3")],
        },
        "vec3_scale" => Spec {
            inputs: &[("value", "vec3"), ("factor", "number")],
            outputs: &[("value", "vec3")],
        },
        "vec3_dot" | "vec3_distance" => Spec {
            inputs: &[("a", "vec3"), ("b", "vec3")],
            outputs: &[("value", "number")],
        },
        "vec3_length" => Spec {
            inputs: &[("value", "vec3")],
            outputs: &[("value", "number")],
        },
        "vec3_normalize" => Spec {
            inputs: &[("value", "vec3")],
            outputs: &[("value", "vec3")],
        },
        "vec3_lerp" => Spec {
            inputs: &[("a", "vec3"), ("b", "vec3"), ("t", "number")],
            outputs: &[("value", "vec3")],
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

    fn input(&self, id: &str, port: &str, stack: &mut HashSet<String>, scope: &[&str]) -> Result<String, String> {
        let (from, output) = self
            .incoming
            .get(&(id, port))
            .ok_or_else(|| format!("Missing input {port} on {id}"))?;
        self.value(from, output, stack, scope)
    }

    fn value(&self, id: &str, output: &str, stack: &mut HashSet<String>, scope: &[&str]) -> Result<String, String> {
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
            "contact_other_id" => "(ctx.other and ctx.other.instance_id or '')".into(),
            "read_field" => format!("ctx:read(ctx.self.{}.ref_{})", self.graph.name, id),
            "get_state" => format!("ctx:get_state({}, 0)", lua_string(state_key(node)?)),
            "for_range" => {
                if output != "index" || !scope.contains(&id) {
                    return Err("For Range index is only available inside its loop body".into());
                }
                format!("_loop_index_{id}")
            }
            "display" => self.input(id, "value", stack, scope)?,
            "add" | "subtract" | "multiply" | "divide" | "modulo" | "less" | "greater"
            | "and" | "or" | "min" | "max" => {
                let a = self.input(id, "a", stack, scope)?;
                let b = self.input(id, "b", stack, scope)?;
                let op = match node.kind.as_str() {
                    "add" => "+",
                    "subtract" => "-",
                    "multiply" => "*",
                    "divide" => "/",
                    "modulo" => "%",
                    "less" => "<",
                    "greater" => ">",
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
            "not" => format!("(not {})", self.input(id, "a", stack, scope)?),
            "ceil" => format!("math.ceil({})", self.input(id, "value", stack, scope)?),
            "abs" | "floor" | "sin" | "cos" => format!(
                "ctx:{}({})", node.kind, self.input(id, "value", stack, scope)?
            ),
            "clamp" | "lerp" | "vec3_make" | "vec3_add" | "vec3_subtract"
            | "vec3_scale" | "vec3_dot" | "vec3_cross" | "vec3_length"
            | "vec3_normalize" | "vec3_distance" | "vec3_lerp"
            | "negate" | "sqrt" | "power" | "round" | "sign" | "tan" | "asin" | "acos"
            | "atan2" | "log" | "exp" | "remap" | "smoothstep" | "ping_pong"
            | "equal" | "less_equal" | "greater_equal" | "not_equal" | "xor" | "parse_number"
            | "number_to_bool" | "bool_to_number" | "string_length" | "string_contains"
            | "string_starts_with" | "string_ends_with" | "string_slice" | "vec3_angle"
            | "vec3_reflect" | "vec3_move_towards" | "color_make" | "color_lerp" => {
                let arguments = spec(&node.kind).unwrap().inputs.iter()
                    .map(|(port, _)| self.input(id, port, stack, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                format!("ctx:{}({})", node.kind, arguments.join(", "))
            }
            "vec3_split" => format!(
                "ctx:vec3_{}({})", output, self.input(id, "value", stack, scope)?
            ),
            "color_split" => format!(
                "ctx:color_{}({})", output, self.input(id, "value", stack, scope)?
            ),
            "to_string" => format!("tostring({})", self.input(id, "value", stack, scope)?),
            "join" => format!(
                "({} .. {})",
                self.input(id, "a", stack, scope)?,
                self.input(id, "b", stack, scope)?
            ),
            "select" => format!(
                "(if ctx:condition({}) then {} else {})",
                self.input(id, "test", stack, scope)?,
                self.input(id, "yes", stack, scope)?,
                self.input(id, "no", stack, scope)?
            ),
            "rotation_y" => format!(
                "ctx:rotation_y({}, {})",
                self.input(id, "degrees", stack, scope)?,
                self.input(id, "base", stack, scope)?
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
        scope: &[&str],
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
                    let test = self.input(next, "test", &mut HashSet::new(), scope)?;
                    code.push_str(&format!(
                        "if ctx:condition({test}) then\n{}else\n{}end\n",
                        self.actions(next, "yes", drive, capture_allowed, stack, scope)?,
                        self.actions(next, "no", drive, capture_allowed, stack, scope)?
                    ));
                }
                "sequence" => {
                    code.push_str(&self.actions(next, "first", drive, capture_allowed, stack, scope)?);
                    code.push_str(&self.actions(next, "second", drive, capture_allowed, stack, scope)?);
                }
                "for_range" | "while_loop" => {
                    let mut nested = scope.to_vec();
                    nested.push(next);
                    if node.kind == "for_range" {
                        let start = self.input(next, "start", &mut HashSet::new(), scope)?;
                        let end = self.input(next, "end", &mut HashSet::new(), scope)?;
                        let step = self.input(next, "step", &mut HashSet::new(), scope)?;
                        let body = self.actions(next, "body", drive, capture_allowed, stack, &nested)?;
                        code.push_str(&format!(
                            "do\nlocal _start_{next}, _end_{next}, _step_{next} = {start}, {end}, {step}\nlocal _count_{next} = ctx:for_count(_start_{next}, _end_{next}, _step_{next})\nlocal _range_index_{next} = _start_{next}\nfor _counter_{next} = 1, _count_{next} do\nlocal _loop_index_{next} = _range_index_{next}\n_range_index_{next} += _step_{next}\nctx:loop_step()\n{body}end\nend\n"
                        ));
                    } else {
                        let test = self.input(next, "test", &mut HashSet::new(), scope)?;
                        let body = self.actions(next, "body", drive, capture_allowed, stack, &nested)?;
                        code.push_str(&format!("while ctx:condition({test}) do\nctx:loop_step()\n{body}end\n"));
                    }
                    code.push_str(&self.actions(next, "done", drive, capture_allowed, stack, scope)?);
                }
                "break_loop" | "continue_loop" => {
                    if scope.is_empty() { return Err("Break and Continue require a loop body".into()); }
                    // A lexical block keeps the terminator last in its block even
                    // when another outgoing flow edge emits subsequent statements.
                    code.push_str(if node.kind == "break_loop" { "do break end\n" } else { "do continue end\n" });
                }
                "write_field" | "drive_field" => {
                    if (node.kind == "drive_field") != drive {
                        return Err(
                            "Drive belongs under On Drive; Write belongs under On Update/Interact"
                                .into(),
                        );
                    }
                    let value = self.input(next, "value", &mut HashSet::new(), scope)?;
                    code.push_str(&format!(
                        "ctx:{}(ctx.self.{}.ref_{}, {value})\n",
                        node.kind, self.graph.name, next
                    ));
                    code.push_str(&self.actions(next, "next", drive, capture_allowed, stack, scope)?);
                }
                "set_state" => {
                    if drive {
                        return Err("Set State cannot run under On Drive".into());
                    }
                    let value = self.input(next, "value", &mut HashSet::new(), scope)?;
                    code.push_str(&format!(
                        "ctx:set_state({}, {value})\n",
                        lua_string(state_key(node)?)
                    ));
                    code.push_str(&self.actions(next, "next", drive, capture_allowed, stack, scope)?);
                }
                "capture_camera" => {
                    if !capture_allowed { return Err("Capture Camera cannot run under On Drive".into()); }
                    code.push_str("ctx:capture_camera(ctx.self)\n");
                    code.push_str(&self.actions(next, "next", drive, capture_allowed, stack, scope)?);
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
            "on_update" | "on_interact" | "on_button_press" | "on_button_release" | "on_contact_start" | "on_contact_end" | "on_drive" => {
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
        return Err("Graph needs an event node".into());
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
        ("on_button_press", "on_button_press", false),
        ("on_button_release", "on_button_release", false),
        ("on_contact_start", "on_contact_start", false),
        ("on_contact_end", "on_contact_end", false),
        ("on_drive", "drive", true),
    ] {
        if let Some(node) = graph.nodes.iter().find(|n| n.kind == kind) {
            let actions = compiler.actions(&node.id, "flow", drive, !drive, &mut HashSet::new(), &[])?;
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
    fn button_and_contact_nodes_compile_and_expose_the_other_slot() {
        let graph = json!({"name":"Events","nodes":[
            {"id":"press","kind":"on_button_press","x":0,"y":0},
            {"id":"release","kind":"on_button_release","x":0,"y":100},
            {"id":"start","kind":"on_contact_start","x":0,"y":200},
            {"id":"end","kind":"on_contact_end","x":0,"y":300},
            {"id":"state","kind":"set_state","x":200,"y":200,"value":"other"},
            {"id":"other","kind":"contact_other_id","x":0,"y":400}
        ],"links":[
            {"from":"start","out":"flow","to":"state","input":"flow"},
            {"from":"other","out":"value","to":"state","input":"value"}
        ]});
        let graph_source = source(graph);
        let program = Program::compile(&graph_source).unwrap();
        assert!(program.schema.has_button_press);
        assert!(program.schema.has_button_release);
        assert!(program.schema.has_contact_start);
        assert!(program.schema.has_contact_end);
        let component = ScriptComponent {
            id: "1".into(), revision: "0".into(), enabled: true, source: graph_source,
            schema: program.schema.clone(), parameters: parameters(&program.schema, &Default::default()).unwrap(),
            state: Default::default(), clock: ScriptClock::default(),
        };
        let object: ObjectView = serde_json::from_value(json!({
            "address":{"instance_id":"00000000-0000-0000-0000-000000000001","local_id":1},
            "object_revision":"0","world_position":[0,0,0],"fields":[],"scripts":[component.clone()]
        })).unwrap();
        let other = kokoverse_protocol::Address { instance_id: "00000000-0000-0000-0000-000000000002".into(), local_id: 1 };
        let outcome = program.run_at_with_event(&object, &component, Event::ContactStart, 0.0, 0.0, &Default::default(), Some(&other)).unwrap();
        assert_eq!(outcome.state["other"], json!(other.instance_id));
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
            "ceil",
            "abs", "floor", "sin", "cos", "clamp", "lerp", "vec3_make", "vec3_split",
            "vec3_add", "vec3_subtract", "vec3_scale", "vec3_dot", "vec3_cross",
            "vec3_length", "vec3_normalize", "vec3_distance", "vec3_lerp",
            "select",
            "to_string",
            "join",
            "rotation_y",
            "display",
            "negate", "sqrt", "power", "round", "sign", "tan", "asin", "acos", "atan2", "log", "exp",
            "remap", "smoothstep", "ping_pong", "less_equal", "greater_equal", "not_equal", "xor",
            "parse_number", "number_to_bool", "bool_to_number", "string_length", "string_contains",
            "string_starts_with", "string_ends_with", "string_slice", "vec3_angle", "vec3_reflect",
            "vec3_move_towards", "color_make", "color_split", "color_lerp",
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
                json!({"from":"v","out":spec(kind).unwrap().outputs[0].0,"to":"a","input":"value"}),
            ];
            for (index, (port, ty)) in spec(kind).unwrap().inputs.iter().enumerate() {
                let (source_kind, default) = match *ty {
                    "bool" => ("bool", json!(true)),
                    "string" => ("string", json!("x")),
                    "vec3" => ("vec3", json!([1, 2, 3])),
                    "color" => ("color", json!([0.1, 0.2, 0.3, 0.4])),
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

    fn evaluate_math(kind: &str, inputs: &[Value], output: &str) -> Result<Value, String> {
        let mut nodes = vec![
            json!({"id":"event","kind":"on_update","x":0,"y":0}),
            json!({"id":"state","kind":"set_state","x":0,"y":0,"value":"result"}),
            json!({"id":"math","kind":kind,"x":0,"y":0}),
        ];
        let mut links = vec![
            json!({"from":"event","out":"flow","to":"state","input":"flow"}),
            json!({"from":"math","out":output,"to":"state","input":"value"}),
        ];
        for (index, ((port, ty), value)) in spec(kind).unwrap().inputs.iter().zip(inputs).enumerate() {
            let id = format!("input{index}");
            let literal_kind = if *ty == "any" {
                match value {
                    Value::Bool(_) => "bool", Value::String(_) => "string",
                    Value::Array(values) if values.len() == 4 => "color",
                    Value::Array(_) => "vec3", _ => "number",
                }
            } else { *ty };
            nodes.push(json!({"id":id,"kind":literal_kind,"x":0,"y":0,"value":value}));
            links.push(json!({"from":id,"out":"value","to":"math","input":port}));
        }
        let graph_source = source(json!({"name":"MathTest","nodes":nodes,"links":links}));
        let program = Program::compile(&graph_source)?;
        let component = ScriptComponent {
            id: "1".into(), revision: "0".into(), enabled: true, source: graph_source,
            schema: program.schema.clone(), parameters: Default::default(),
            state: Default::default(), clock: ScriptClock::default(),
        };
        let object: ObjectView = serde_json::from_value(json!({
            "address":{"instance_id":"00000000-0000-0000-0000-000000000001","local_id":1},
            "object_revision":"0","world_position":[0,0,0],"fields":[],"scripts":[component.clone()]
        })).unwrap();
        Ok(program.run(&object, &component, Event::Update, 0.05)?.state["result"].clone())
    }

    #[test]
    fn scalar_math_nodes_produce_values_and_validate_ranges() {
        for (kind, inputs, expected) in [
            ("abs", vec![json!(-3.5)], json!(3.5)),
            ("floor", vec![json!(-1.25)], json!(-2.0)),
            ("sin", vec![json!(std::f64::consts::FRAC_PI_2)], json!(1.0)),
            ("cos", vec![json!(std::f64::consts::PI)], json!(-1.0)),
            ("clamp", vec![json!(-2), json!(0), json!(1)], json!(0.0)),
            ("clamp", vec![json!(2), json!(0), json!(1)], json!(1.0)),
            ("lerp", vec![json!(2), json!(10), json!(0.25)], json!(4.0)),
            ("lerp", vec![json!(2), json!(10), json!(-1)], json!(2.0)),
            ("lerp", vec![json!(2), json!(10), json!(2)], json!(10.0)),
        ] {
            assert_eq!(evaluate_math(kind, &inputs, "value").unwrap().as_f64(), expected.as_f64(), "{kind}");
        }
        assert!(evaluate_math("clamp", &[json!(0), json!(2), json!(1)], "value").unwrap_err().contains("minimum"));
    }

    #[test]
    fn new_unary_nodes_reject_numeric_strings_from_any_ports() {
        for kind in ["abs", "floor", "sin", "cos"] {
            let graph_source = source(json!({"name":"StrictMath","nodes":[
                {"id":"event","kind":"on_update","x":0,"y":0},
                {"id":"state","kind":"set_state","x":0,"y":0,"value":"result"},
                {"id":"math","kind":kind,"x":0,"y":0},
                {"id":"input","kind":"get_state","x":0,"y":0,"value":"input"}
            ],"links":[
                {"from":"event","out":"flow","to":"state","input":"flow"},
                {"from":"math","out":"value","to":"state","input":"value"},
                {"from":"input","out":"value","to":"math","input":"value"}
            ]}));
            let program = Program::compile(&graph_source).unwrap();
            let component = ScriptComponent {
                id: "1".into(), revision: "0".into(), enabled: true, source: graph_source,
                schema: program.schema.clone(), parameters: Default::default(),
                state: std::collections::BTreeMap::from([("input".into(),json!("3"))]), clock: ScriptClock::default(),
            };
            let object: ObjectView = serde_json::from_value(json!({
                "address":{"instance_id":"00000000-0000-0000-0000-000000000001","local_id":1},
                "object_revision":"0","world_position":[0,0,0],"fields":[],"scripts":[component.clone()]
            })).unwrap();
            let error = program.run(&object,&component,Event::Update,0.05).unwrap_err();
            assert!(error.contains("finite number"), "{kind}: {error}");
        }
    }

    #[test]
    fn vector_math_nodes_evaluate_and_preserve_all_split_outputs() {
        for (kind, inputs, expected) in [
            ("vec3_make", vec![json!(2), json!(4), json!(8)], json!([2.0,4.0,8.0])),
            ("vec3_add", vec![json!([1,2,3]), json!([4,5,6])], json!([5.0,7.0,9.0])),
            ("vec3_subtract", vec![json!([1,2,3]), json!([4,5,6])], json!([-3.0,-3.0,-3.0])),
            ("vec3_scale", vec![json!([1,2,3]), json!(2)], json!([2.0,4.0,6.0])),
            ("vec3_dot", vec![json!([1,2,3]), json!([4,5,6])], json!(32.0)),
            ("vec3_cross", vec![json!([1,2,3]), json!([4,5,6])], json!([-3.0,6.0,-3.0])),
            ("vec3_length", vec![json!([3,4,0])], json!(5.0)),
            ("vec3_normalize", vec![json!([0,3,4])], json!([0.0,0.6,0.8])),
            ("vec3_normalize", vec![json!([0,0,0])], json!([0.0,0.0,0.0])),
            ("vec3_distance", vec![json!([1,2,3]), json!([4,6,3])], json!(5.0)),
            ("vec3_lerp", vec![json!([1,2,3]), json!([4,6,3]), json!(0.25)], json!([1.75,3.0,3.0])),
        ] {
            let actual = evaluate_math(kind, &inputs, "value").unwrap();
            if expected.is_number() {
                assert_eq!(actual.as_f64(), expected.as_f64(), "{kind}");
            } else {
                assert_eq!(actual, expected, "{kind}");
            }
        }
        for (port, expected) in [("x", 2.0), ("y", 4.0), ("z", 8.0)] {
            assert_eq!(evaluate_math("vec3_split", &[json!([2,4,8])], port).unwrap().as_f64(), Some(expected));
        }
        assert!(evaluate_math("vec3_split", &[json!([2,4,8])], "value").unwrap_err().contains("output port"));
    }

    #[test]
    fn vector_nodes_reject_wrong_types_and_missing_inputs() {
        let mut graph = json!({"name":"BadMath","nodes":[
            {"id":"event","kind":"on_update","x":0,"y":0},
            {"id":"state","kind":"set_state","x":0,"y":0,"value":"result"},
            {"id":"math","kind":"vec3_length","x":0,"y":0},
            {"id":"number","kind":"number","x":0,"y":0,"value":3}
        ],"links":[
            {"from":"event","out":"flow","to":"state","input":"flow"},
            {"from":"math","out":"value","to":"state","input":"value"},
            {"from":"number","out":"value","to":"math","input":"value"}
        ]});
        assert!(Program::compile(&source(graph.clone())).err().unwrap().contains("Incompatible ports"));
        graph["links"].as_array_mut().unwrap().pop();
        assert!(Program::compile(&source(graph)).err().unwrap().contains("Missing input value"));
    }

    #[test]
    fn repeated_vector_inputs_still_obey_graph_expansion_budget() {
        let mut nodes = vec![
            json!({"id":"event","kind":"on_update","x":0,"y":0}),
            json!({"id":"state","kind":"set_state","x":0,"y":0,"value":"result"}),
            json!({"id":"v0","kind":"vec3","x":0,"y":0,"value":[1,2,3]}),
        ];
        let mut links = vec![json!({"from":"event","out":"flow","to":"state","input":"flow"})];
        for index in 1..=9 {
            let id = format!("v{index}");
            let previous = format!("v{}",index-1);
            nodes.push(json!({"id":id,"kind":"vec3_add","x":0,"y":0}));
            for input in ["a","b"] {
                links.push(json!({"from":previous,"out":"value","to":id,"input":input}));
            }
        }
        links.push(json!({"from":"v9","out":"value","to":"state","input":"value"}));
        let graph = json!({"name":"Expansion","nodes":nodes,"links":links});
        assert!(Program::compile(&source(graph)).err().unwrap().contains("too many operations"));
    }

    fn run_control(graph: Value) -> Result<crate::Output, String> {
        let graph_source = source(graph);
        let program = Program::compile(&graph_source)?;
        let component = ScriptComponent {
            id: "1".into(), revision: "0".into(), enabled: true, source: graph_source,
            schema: program.schema.clone(), parameters: parameters(&program.schema,&Default::default()).unwrap(),
            state: Default::default(), clock: ScriptClock::default(),
        };
        let object: ObjectView = serde_json::from_value(json!({
            "address":{"instance_id":"00000000-0000-0000-0000-000000000001","local_id":1},
            "object_revision":"0","world_position":[0,0,0],"fields":[],"scripts":[component.clone()]
        })).unwrap();
        let result = program.run(&object,&component,Event::Update,0.05);
        assert!(component.state.is_empty(),"A callback modified its saved input state");
        result
    }

    fn range_graph(start: i32, end: i32, step: i32) -> Value {
        json!({"name":"Range","nodes":[
            {"id":"event","kind":"on_update","x":0,"y":0},
            {"id":"range","kind":"for_range","x":0,"y":0},
            {"id":"start","kind":"number","x":0,"y":0,"value":start},
            {"id":"end","kind":"number","x":0,"y":0,"value":end},
            {"id":"step","kind":"number","x":0,"y":0,"value":step},
            {"id":"read","kind":"get_state","x":0,"y":0,"value":"sum"},
            {"id":"sum","kind":"add","x":0,"y":0},
            {"id":"write","kind":"set_state","x":0,"y":0,"value":"sum"},
            {"id":"done","kind":"set_state","x":0,"y":0,"value":"done"},
            {"id":"true","kind":"bool","x":0,"y":0,"value":true}
        ],"links":[
            {"from":"event","out":"flow","to":"range","input":"flow"},
            {"from":"start","out":"value","to":"range","input":"start"},
            {"from":"end","out":"value","to":"range","input":"end"},
            {"from":"step","out":"value","to":"range","input":"step"},
            {"from":"range","out":"body","to":"write","input":"flow"},
            {"from":"read","out":"value","to":"sum","input":"a"},
            {"from":"range","out":"index","to":"sum","input":"b"},
            {"from":"sum","out":"value","to":"write","input":"value"},
            {"from":"range","out":"done","to":"done","input":"flow"},
            {"from":"true","out":"value","to":"done","input":"value"}
        ]})
    }

    #[test]
    fn for_ranges_accumulate_indices_and_handle_descending_empty_and_zero_ranges() {
        for (start,end,step,expected) in [(1,5,1,Some(15)),(5,1,-2,Some(9)),(0,0,1,Some(0)),(5,1,1,None),(1,5,-1,None),(0,63,1,Some(2016))] {
            let output = run_control(range_graph(start,end,step)).unwrap();
            assert_eq!(output.state.get("sum").and_then(Value::as_i64),expected,"{start}..{end} step {step}");
            assert_eq!(output.state["done"],json!(true));
        }
        assert!(run_control(range_graph(0,1,0)).unwrap_err().contains("step"));
        assert!(run_control(range_graph(0,64,1)).unwrap_err().contains("64 iterations"));
    }

    fn while_graph(limit: i32) -> Value {
        let mut graph = range_graph(0,0,1);
        graph["name"] = json!("While");
        graph["nodes"][1]["kind"] = json!("while_loop");
        graph["nodes"][2]["id"] = json!("limit");
        graph["nodes"][2]["value"] = json!(limit);
        graph["nodes"][4]["value"] = json!(1);
        graph["nodes"].as_array_mut().unwrap().push(json!({"id":"test","kind":"less","x":0,"y":0}));
        let links = graph["links"].as_array_mut().unwrap();
        links.retain(|link| !(link["to"] == "range" && link["input"] != "flow") && !(link["from"] == "range" && link["out"] == "index"));
        links.extend([
            json!({"from":"read","out":"value","to":"test","input":"a"}),
            json!({"from":"limit","out":"value","to":"test","input":"b"}),
            json!({"from":"test","out":"value","to":"range","input":"test"}),
            json!({"from":"step","out":"value","to":"sum","input":"b"}),
        ]);
        graph
    }

    #[test]
    fn while_rechecks_changed_state_and_aborts_the_whole_proposal_at_limit() {
        let output = run_control(while_graph(3)).unwrap();
        assert_eq!(output.state["sum"],json!(3));
        assert_eq!(output.state["done"],json!(true));
        let error = run_control(while_graph(65)).unwrap_err();
        assert!(error.contains("64 iterations"),"{error}");
        let mut bad = while_graph(1);
        bad["links"].as_array_mut().unwrap().iter_mut().find(|link| link["to"] == "range" && link["input"] == "test").unwrap()["from"] = json!("read");
        assert!(run_control(bad).unwrap_err().contains("boolean"));
    }

    #[test]
    fn break_and_continue_skip_remaining_body_and_done_still_runs() {
        let mut graph = range_graph(0,5,1);
        graph["nodes"].as_array_mut().unwrap().extend([
            json!({"id":"two","kind":"number","x":0,"y":0,"value":2}),
            json!({"id":"four","kind":"number","x":0,"y":0,"value":4}),
            json!({"id":"is_two","kind":"equal","x":0,"y":0}),
            json!({"id":"is_four","kind":"equal","x":0,"y":0}),
            json!({"id":"skip","kind":"branch","x":0,"y":0}),
            json!({"id":"stop","kind":"branch","x":0,"y":0}),
            json!({"id":"continue","kind":"continue_loop","x":0,"y":0}),
            json!({"id":"break","kind":"break_loop","x":0,"y":0}),
        ]);
        let links = graph["links"].as_array_mut().unwrap();
        links.iter_mut().find(|link| link["from"] == "range" && link["out"] == "body").unwrap()["to"] = json!("skip");
        for (value,compare,branch) in [("two","is_two","skip"),("four","is_four","stop")] {
            links.extend([
                json!({"from":"range","out":"index","to":compare,"input":"a"}),
                json!({"from":value,"out":"value","to":compare,"input":"b"}),
                json!({"from":compare,"out":"value","to":branch,"input":"test"}),
            ]);
        }
        links.extend([
            json!({"from":"skip","out":"yes","to":"continue","input":"flow"}),
            json!({"from":"skip","out":"no","to":"stop","input":"flow"}),
            json!({"from":"stop","out":"yes","to":"break","input":"flow"}),
            json!({"from":"stop","out":"no","to":"write","input":"flow"}),
        ]);
        let output = run_control(graph).unwrap();
        assert_eq!(output.state["sum"],json!(4));
        assert_eq!(output.state["done"],json!(true));
    }

    fn nested_ranges(inner_end: i32) -> Value {
        let mut graph = range_graph(0,7,1);
        graph["nodes"].as_array_mut().unwrap().extend([
            json!({"id":"inner","kind":"for_range","x":0,"y":0}),
            json!({"id":"inner_end","kind":"number","x":0,"y":0,"value":inner_end}),
        ]);
        let links = graph["links"].as_array_mut().unwrap();
        links.iter_mut().find(|link| link["from"] == "range" && link["out"] == "body").unwrap()["to"] = json!("inner");
        links.iter_mut().find(|link| link["to"] == "sum" && link["input"] == "b").unwrap()["from"] = json!("step");
        links.iter_mut().find(|link| link["to"] == "sum" && link["input"] == "b").unwrap()["out"] = json!("value");
        links.extend([
            json!({"from":"start","out":"value","to":"inner","input":"start"}),
            json!({"from":"inner_end","out":"value","to":"inner","input":"end"}),
            json!({"from":"step","out":"value","to":"inner","input":"step"}),
            json!({"from":"inner","out":"body","to":"write","input":"flow"}),
        ]);
        graph
    }

    #[test]
    fn nested_loops_share_one_budget_and_break_targets_innermost_loop() {
        assert_eq!(run_control(nested_ranges(6)).unwrap().state["sum"],json!(56));
        assert!(run_control(nested_ranges(7)).unwrap_err().contains("64 iterations"));
        let mut graph = nested_ranges(6);
        graph["nodes"].as_array_mut().unwrap().push(json!({"id":"break","kind":"break_loop","x":0,"y":0}));
        let links = graph["links"].as_array_mut().unwrap();
        links.iter_mut().find(|link| link["from"] == "inner" && link["out"] == "body").unwrap()["to"] = json!("break");
        links.push(json!({"from":"inner","out":"done","to":"write","input":"flow"}));
        assert_eq!(run_control(graph).unwrap().state["sum"],json!(8));
    }

    #[test]
    fn loop_scope_and_drive_write_rules_are_checked_at_compile_time() {
        for kind in ["break_loop","continue_loop"] {
            let graph = json!({"name":"BadScope","nodes":[
                {"id":"event","kind":"on_update","x":0,"y":0},
                {"id":"control","kind":kind,"x":0,"y":0}
            ],"links":[{"from":"event","out":"flow","to":"control","input":"flow"}]});
            assert!(Program::compile(&source(graph)).err().unwrap().contains("loop body"));
        }
        let mut bad = range_graph(0,2,1);
        bad["links"].as_array_mut().unwrap().iter_mut().find(|link| link["to"] == "done" && link["input"] == "value").unwrap().clone_from(&json!({"from":"range","out":"index","to":"done","input":"value"}));
        assert!(Program::compile(&source(bad)).err().unwrap().contains("inside its loop body"));
        let mut drive = range_graph(0,2,1);
        drive["nodes"][0]["kind"] = json!("on_drive");
        assert!(Program::compile(&source(drive.clone())).err().unwrap().contains("Set State cannot"));
        drive["nodes"][7] = json!({"id":"write","kind":"write_field","x":0,"y":0});
        assert!(Program::compile(&source(drive)).err().unwrap().contains("Drive belongs"));
    }

    #[test]
    fn new_operator_ports_and_color_split_compile_to_the_right_helpers() {
        for (kind,inputs,expected) in [
            ("atan2",vec![json!(1),json!(0)],json!(std::f64::consts::FRAC_PI_2)),
            ("remap",vec![json!(0.25),json!(0),json!(1),json!(10),json!(20)],json!(12.5)),
            ("smoothstep",vec![json!(0),json!(1),json!(0.5)],json!(0.5)),
            ("string_length",vec![json!("한A🙂")],json!(3)),
            ("string_slice",vec![json!("한A🙂"),json!(1),json!(2)],json!("A🙂")),
            ("string_contains",vec![json!("hello"),json!("ell")],json!(true)),
            ("equal",vec![json!([1,2,3]),json!([1,2,3])],json!(true)),
            ("not_equal",vec![json!([1,2,3]),json!([1,2,4])],json!(true)),
            ("color_make",vec![json!(0.1),json!(0.2),json!(0.3),json!(0.4)],json!([0.1,0.2,0.3,0.4])),
        ] {
            let actual = evaluate_math(kind,&inputs,"value").unwrap();
            if expected.is_number() { assert_eq!(actual.as_f64(),expected.as_f64(),"{kind}"); }
            else { assert_eq!(actual,expected,"{kind}"); }
        }
        for (port,expected) in [("r",0.1),("g",0.2),("b",0.3),("a",0.4)] {
            assert_eq!(evaluate_math("color_split",&[json!([0.1,0.2,0.3,0.4])],port).unwrap().as_f64(),Some(expected));
        }
    }
}
