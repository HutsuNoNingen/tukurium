//! Restricted Luau programs. A call produces a proposal; it never owns an ECS World.
use kokoverse_protocol::{ObjectView, ScriptComponent, ScriptField, ScriptSchema, TypedReference};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue, VmState, chunk::ChunkMode};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
mod graph;

pub const MAX_SOURCE: usize = 8 * 1024;
pub const MAX_SCRIPTS_PER_OBJECT: usize = 4;
pub const MAX_SCRIPTS_WORLD: usize = 32;
const MAX_FIELDS: usize = 16;
const MAX_COMMANDS: usize = 128;

#[derive(Clone, Copy)]
pub enum Event {
    Update,
    Interact,
    Drive,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
    pub color: [f64; 4],
    pub state: BTreeMap<String, Value>,
    pub drives: BTreeMap<String, Value>,
    pub field_writes: Vec<(TypedReference, Value)>,
    pub field_drives: Vec<(TypedReference, Value)>,
    commands: usize,
}

struct Budget {
    calls: AtomicUsize,
    deadline: Mutex<Instant>,
}
impl Budget {
    fn reset(&self) {
        self.calls.store(0, Ordering::Relaxed);
        *self.deadline.lock().unwrap() = Instant::now() + Duration::from_millis(5);
    }
}

pub struct Program {
    lua: Lua,
    module: Table,
    budget: Arc<Budget>,
    pub schema: ScriptSchema,
}

fn error(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}
fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
}

pub fn validate_atom(value: &Value) -> bool {
    match value {
        Value::Bool(_) => true,
        Value::Number(n) => n.as_f64().is_some_and(|n| n.is_finite() && n.abs() <= 1e9),
        Value::String(s) => s.len() <= 512,
        Value::Array(a) => {
            (2..=4).contains(&a.len())
                && a.iter()
                    .all(|v| v.as_f64().is_some_and(|n| n.is_finite() && n.abs() <= 1e9))
        }
        _ => false,
    }
}

pub fn validate_state(state: &BTreeMap<String, Value>) -> Result<(), String> {
    if state.len() > MAX_FIELDS
        || state
            .iter()
            .any(|(key, value)| !identifier(key) || !validate_atom(value))
        || serde_json::to_vec(state).map_err(|e| e.to_string())?.len() > 4096
    {
        return Err("Script state must contain up to 16 small named values".into());
    }
    Ok(())
}

fn field_valid(field: &ScriptField, value: &Value) -> bool {
    if field.value_type == "field_ref" {
        return value.is_null() || serde_json::from_value::<kokoverse_protocol::TypedReference>(value.clone()).is_ok_and(|r| matches!(r,kokoverse_protocol::TypedReference::Field{ref target,ref field_id} if target.instance_id.len()==36 && target.local_id>0 && field_id.len()<=80));
    }
    if !validate_atom(value) {
        return false;
    }
    match field.value_type.as_str() {
        "number" => value.as_f64().is_some_and(|n| {
            field.min.is_none_or(|min| n >= min) && field.max.is_none_or(|max| n <= max)
        }),
        "bool" => value.is_boolean(),
        "string" => value.is_string(),
        "vec3" => value.as_array().is_some_and(|a| a.len() == 3),
        "color" => value.as_array().is_some_and(|a| {
            a.len() == 4
                && a.iter()
                    .all(|v| v.as_f64().is_some_and(|n| (0.0..=1.0).contains(&n)))
        }),
        _ => false,
    }
}

pub fn parameters(
    schema: &ScriptSchema,
    supplied: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, String> {
    if supplied
        .keys()
        .any(|key| !schema.fields.iter().any(|f| &f.id == key))
    {
        return Err("Unknown script parameter".into());
    }
    schema
        .fields
        .iter()
        .map(|field| {
            let value = supplied.get(&field.id).unwrap_or(&field.default);
            if !field_valid(field, value) {
                return Err(format!("Invalid script parameter: {}", field.id));
            }
            Ok((field.id.clone(), value.clone()))
        })
        .collect()
}

pub fn validate_component(component: &ScriptComponent) -> Result<(), String> {
    if !component.clock.anchor.is_finite()
        || component.clock.anchor < 0.0
        || !component.clock.elapsed.is_finite()
        || component.clock.elapsed < 0.0
        || component.source.is_empty()
        || component.source.len() > MAX_SOURCE
        || !identifier(&component.schema.name)
        || ["Transform", "Renderer", "Collider", "State"].contains(&component.schema.name.as_str())
        || component.schema.fields.len() > MAX_FIELDS
    {
        return Err("Invalid script component".into());
    }
    let mut names = std::collections::HashSet::new();
    for field in &component.schema.fields {
        if !identifier(&field.id)
            || !names.insert(&field.id)
            || field.label.len() > 80
            || !field_valid(field, &field.default)
            || field.min.is_some_and(|n| !n.is_finite())
            || field.max.is_some_and(|n| !n.is_finite())
            || field.min.zip(field.max).is_some_and(|(min, max)| min > max)
        {
            return Err("Invalid script schema".into());
        }
    }
    if parameters(&component.schema, &component.parameters)? != component.parameters {
        return Err("Missing script parameters".into());
    }
    validate_state(&component.state)
}

fn atom(value: LuaValue) -> mlua::Result<Value> {
    let value = match value {
        LuaValue::Boolean(v) => json!(v),
        LuaValue::Integer(v) => json!(v),
        LuaValue::Number(v) if v.is_finite() => json!(v),
        LuaValue::String(v) => json!(v.to_str()?.to_string()),
        LuaValue::Table(table) if (2..=4).contains(&table.raw_len()) => {
            let mut values = Vec::new();
            for i in 1..=table.raw_len() {
                values.push(table.raw_get::<f64>(i)?);
            }
            json!(values)
        }
        _ => {
            return Err(error(
                "Expected a number, boolean, short string or numeric vector",
            ));
        }
    };
    if !validate_atom(&value) {
        return Err(error("Script value is too large or not finite"));
    }
    Ok(value)
}

fn vector<const N: usize>(table: Table) -> mlua::Result<[f64; N]> {
    if table.raw_len() != N {
        return Err(error(format!("Expected {N} numbers")));
    }
    let mut result = [0.0_f64; N];
    for (i, value) in result.iter_mut().enumerate() {
        *value = table.raw_get(i + 1)?;
        if !value.is_finite() {
            return Err(error("Value must be finite"));
        }
    }
    Ok(result)
}

impl Program {
    pub fn compile(source: &str) -> Result<Self, String> {
        if source.is_empty() || source.len() > MAX_SOURCE {
            return Err("Luau source must be 1–8192 bytes".into());
        }
        let generated = if graph::is_graph(source) {
            Some(graph::compile(source)?)
        } else {
            None
        };
        let source = generated.as_deref().unwrap_or(source);
        let build = || -> mlua::Result<Self> {
            let lua = Lua::new();
            lua.set_memory_limit(4 * 1024 * 1024)?;
            // Protected calls/coroutines could catch a budget error indefinitely.
            for key in [
                "os",
                "io",
                "debug",
                "package",
                "require",
                "loadstring",
                "load",
                "dofile",
                "loadfile",
                "getfenv",
                "setfenv",
                "collectgarbage",
                "pcall",
                "xpcall",
                "coroutine",
                "print",
            ] {
                lua.globals().set(key, LuaValue::Nil)?;
            }
            lua.sandbox(true)?;
            let budget = Arc::new(Budget {
                calls: AtomicUsize::new(0),
                deadline: Mutex::new(Instant::now()),
            });
            let hook = budget.clone();
            lua.set_interrupt(move |_| {
                if hook.calls.fetch_add(1, Ordering::Relaxed) >= 2000
                    || Instant::now() >= *hook.deadline.lock().unwrap()
                {
                    return Err(error("Script execution budget exceeded"));
                }
                Ok(VmState::Continue)
            });
            budget.reset();
            let module: Table = lua
                .load(source)
                .set_name("UGC")
                .set_mode(ChunkMode::Text)
                .eval()?;
            let name: String = module.get("name")?;
            if !identifier(&name)
                || ["Transform", "Renderer", "Collider", "State"].contains(&name.as_str())
            {
                return Err(error(
                    "Use a unique component name (letters, digits, underscores)",
                ));
            }
            let mut fields = Vec::new();
            if let Some(definitions) = module.get::<Option<Table>>("fields")? {
                for entry in definitions.pairs::<String, Table>() {
                    let (id, definition) = entry?;
                    if !identifier(&id) || fields.len() >= MAX_FIELDS {
                        return Err(error("At most 16 named fields are allowed"));
                    }
                    let value_type: String = definition.get("type")?;
                    let default = if value_type == "field_ref" {
                        Value::Null
                    } else {
                        atom(definition.get("default")?)?
                    };
                    let field = ScriptField {
                        label: definition
                            .get::<Option<String>>("label")?
                            .unwrap_or_else(|| id.clone()),
                        id,
                        value_type,
                        default,
                        min: definition.get("min")?,
                        max: definition.get("max")?,
                    };
                    if field.label.len() > 80
                        || field.min.is_some_and(|n| !n.is_finite())
                        || field.max.is_some_and(|n| !n.is_finite())
                        || field.min.zip(field.max).is_some_and(|(min, max)| min > max)
                        || !field_valid(&field, &field.default)
                    {
                        return Err(error("Invalid field type, bounds or default"));
                    }
                    fields.push(field);
                }
            }
            fields.sort_by(|a, b| a.id.cmp(&b.id));
            let mut handlers = 0;
            for name in ["update", "on_interact", "drive"] {
                match module.get::<LuaValue>(name)? {
                    LuaValue::Function(_) => handlers += 1,
                    LuaValue::Nil => {}
                    _ => return Err(error(format!("{name} must be a function"))),
                }
            }
            if handlers == 0 {
                return Err(error(
                    "Define drive(ctx), update(ctx, dt) or on_interact(ctx)",
                ));
            }
            let has_update = module.get::<Option<Function>>("update")?.is_some();
            let has_interact = module.get::<Option<Function>>("on_interact")?.is_some();
            let has_drive = module.get::<Option<Function>>("drive")?.is_some();
            Ok(Self {
                lua,
                module,
                budget,
                schema: ScriptSchema {
                    name,
                    fields,
                    has_update,
                    has_drive,
                    has_interact,
                },
            })
        };
        build().map_err(|e| e.to_string())
    }

    pub fn run(
        &self,
        object: &ObjectView,
        component: &ScriptComponent,
        event: Event,
        dt: f64,
    ) -> Result<Output, String> {
        self.run_at(object, component, event, dt, 0.0)
    }

    pub fn run_at(
        &self,
        object: &ObjectView,
        component: &ScriptComponent,
        event: Event,
        dt: f64,
        world_time: f64,
    ) -> Result<Output, String> {
        self.run_at_with_fields(object, component, event, dt, world_time, &BTreeMap::new())
    }

    pub fn run_at_with_fields(
        &self,
        object: &ObjectView,
        component: &ScriptComponent,
        event: Event,
        dt: f64,
        world_time: f64,
        readable_fields: &BTreeMap<String, Value>,
    ) -> Result<Output, String> {
        let run = || -> mlua::Result<Output> {
            let color: [f64; 4] = object.fields.iter().find(|f| f.id == "color")
                .map(|field| serde_json::from_value(field.value.clone()))
                .transpose()
                .map_err(|e| error(e.to_string()))?
                .unwrap_or([1.0; 4]);
            let output = Arc::new(Mutex::new(Output {
                position: object.world_position,
                rotation: object.world_rotation,
                scale: object.world_scale,
                color,
                state: component.state.clone(),
                drives: BTreeMap::new(),
                field_writes: Vec::new(),
                field_drives: Vec::new(),
                commands: 0,
            }));
            let handler = match event {
                Event::Update => "update",
                Event::Interact => "on_interact",
                Event::Drive => "drive",
            };
            let Some(function) = self.module.get::<Option<Function>>(handler)? else {
                return Ok(output.lock().unwrap().clone());
            };
            let ctx = self.context(
                object,
                component,
                output.clone(),
                matches!(event, Event::Drive),
                world_time,
                readable_fields,
            )?;
            self.budget.reset();
            function.call::<()>((ctx, dt))?;
            let result = output.lock().unwrap().clone();
            validate_state(&result.state).map_err(error)?;
            Ok(result)
        };
        run().map_err(|e| e.to_string())
    }

    fn context(
        &self,
        object: &ObjectView,
        component: &ScriptComponent,
        output: Arc<Mutex<Output>>,
        local_drive: bool,
        world_time: f64,
        readable_fields: &BTreeMap<String, Value>,
    ) -> mlua::Result<Table> {
        let lua = &self.lua;
        let ctx = lua.create_table()?;
        let readable_fields = readable_fields.clone();
        ctx.set(
            "read",
            lua.create_function(move |lua, (_ctx, value): (Table, LuaValue)| {
                let reference: Option<kokoverse_protocol::TypedReference> =
                    lua.from_value(value)?;
                let Some(reference) = reference else {
                    return Ok(LuaValue::Nil);
                };
                match readable_fields.get(&serde_json::to_string(&reference).unwrap()) {
                    Some(value) => lua.to_value(value),
                    None => Ok(LuaValue::Nil),
                }
            })?,
        )?;
        ctx.set("world_time", world_time)?;
        ctx.set("time", component.clock.time(world_time, component.enabled))?;
        ctx.set(
            "rotation_y",
            lua.create_function(|lua, (_ctx, degrees, base): (Table, f64, Option<Table>)| {
                if !degrees.is_finite() {
                    return Err(error("Angle must be finite"));
                }
                let base = if let Some(base) = base {
                    glam::DQuat::from_array(vector::<4>(base)?)
                } else {
                    glam::DQuat::IDENTITY
                };
                let q = (glam::DQuat::from_rotation_y(degrees.rem_euclid(360.0).to_radians())
                    * base)
                    .normalize();
                lua.to_value(&q.to_array())
            })?,
        )?;

        let entity = lua.create_table()?;
        entity.set("id", object.address.instance_id.clone())?;
        for script in &object.scripts {
            entity.set(
                script.schema.name.as_str(),
                lua.to_value(&script.parameters)?,
            )?;
        }
        entity.set("Transform", lua.to_value(&json!({"position":object.world_position,"rotation":object.world_rotation,"scale":object.world_scale}))?)?;
        entity.set(
            "Renderer",
            lua.to_value(&json!({"color":output.lock().unwrap().color}))?,
        )?;
        if let Some(collider) = &object.collider {
            entity.set("Collider", lua.to_value(collider)?)?;
        }
        entity.set_readonly(true);
        let query_entity = entity.clone();
        ctx.set(
            "query",
            lua.create_function(
                move |lua, (_ctx, required): (Table, mlua::Variadic<String>)| {
                    if required.len() > 8 {
                        return Err(error("Too many query components"));
                    }
                    let mut matches = true;
                    for key in required.iter() {
                        if query_entity.raw_get::<LuaValue>(key.as_str())? == LuaValue::Nil {
                            matches = false;
                        }
                    }
                    let entity = query_entity.clone();
                    let mut done = !matches;
                    lua.create_function_mut(move |_, ()| {
                        if done {
                            Ok(None)
                        } else {
                            done = true;
                            Ok(Some(entity.clone()))
                        }
                    })
                },
            )?,
        )?;
        ctx.set("self", entity)?;
        for name in [
            "set_position",
            "translate",
            "set_scale",
            "set_color",
            "rotate_y",
        ] {
            let output = output.clone();
            let expected = object.address.instance_id.clone();
            ctx.set(
                name,
                lua.create_function(move |_, (_ctx, target, value): (Table, Table, LuaValue)| {
                    if local_drive {
                        return Err(error(
                            "Shared writes are not allowed in drive(ctx); use ctx:drive",
                        ));
                    }
                    if target.raw_get::<String>("id")? != expected {
                        return Err(error("Target is outside this script's object"));
                    }
                    let mut out = output.lock().unwrap();
                    out.commands += 1;
                    if out.commands > MAX_COMMANDS {
                        return Err(error("Too many script writes"));
                    }
                    match name {
                        "rotate_y" => {
                            let degrees = match value {
                                LuaValue::Number(n) => n,
                                LuaValue::Integer(n) => n as f64,
                                _ => return Err(error("Expected an angle in degrees")),
                            };
                            if !degrees.is_finite() || degrees.abs() > 36000.0 {
                                return Err(error("Invalid rotation angle"));
                            }
                            let current = glam::DQuat::from_array(out.rotation);
                            out.rotation = (glam::DQuat::from_rotation_y(degrees.to_radians())
                                * current)
                                .normalize()
                                .to_array();
                        }
                        "set_color" => {
                            let LuaValue::Table(value) = value else {
                                return Err(error("Expected RGBA"));
                            };
                            let color = vector::<4>(value)?;
                            if color.iter().any(|v| !(0.0..=1.0).contains(v)) {
                                return Err(error("Color must be between 0 and 1"));
                            }
                            out.color = color;
                        }
                        _ => {
                            let LuaValue::Table(value) = value else {
                                return Err(error("Expected a vector"));
                            };
                            let v = vector::<3>(value)?;
                            match name {
                                "set_position" => out.position = v,
                                "set_scale" => out.scale = v,
                                "translate" => {
                                    for (position, delta) in out.position.iter_mut().zip(v) {
                                        *position += delta;
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                    if out
                        .position
                        .iter()
                        .any(|v| !v.is_finite() || v.abs() > 1000.0)
                        || out
                            .scale
                            .iter()
                            .any(|v| !v.is_finite() || !(0.05..=20.0).contains(v))
                    {
                        return Err(error("Transform is outside world bounds"));
                    }
                    Ok(())
                })?,
            )?;
        }
        for name in ["write", "drive"] {
            let output = output.clone();
            let expected = object.address.instance_id.clone();
            ctx.set(name, lua.create_function(move |_, (_ctx, target, field, value): (Table, Table, String, LuaValue)| {
                if (name == "drive") != local_drive {
                    return Err(error("Use ctx:drive only in drive(ctx), and ctx:write only in shared callbacks"));
                }
                if target.raw_get::<String>("id")? != expected { return Err(error("Target is outside this script's object")); }
                let LuaValue::Table(value) = value else { return Err(error("Expected a numeric vector")); };
                let value = match field.as_str() {
                    "position" => { let v=vector::<3>(value)?; if v.iter().any(|n| n.abs()>1000.0) {return Err(error("Position outside world bounds"));} json!(v) },
                    "scale" => { let v=vector::<3>(value)?; if v.iter().any(|n| !(0.05..=20.0).contains(n)) {return Err(error("Scale outside world bounds"));} json!(v) },
                    "rotation" => { let v=vector::<4>(value)?; if (v.iter().map(|n|n*n).sum::<f64>()-1.0).abs()>0.0001 {return Err(error("Expected a unit quaternion"));} json!(v) },
                    "color" => { let v=vector::<4>(value)?; if v.iter().any(|n| !(0.0..=1.0).contains(n)) {return Err(error("Color outside 0..1"));} json!(v) },
                    _ => return Err(error("Writable fields: position, rotation, scale, color")),
                };
                let mut out=output.lock().unwrap();
                out.commands+=1;
                if out.commands>MAX_COMMANDS {return Err(error("Too many script writes"));}
                if local_drive { out.drives.insert(field,value); }
                else { match field.as_str() {
                    "position" => out.position=serde_json::from_value(value).unwrap(),
                    "rotation" => out.rotation=serde_json::from_value(value).unwrap(),
                    "scale" => out.scale=serde_json::from_value(value).unwrap(),
                    "color" => out.color=serde_json::from_value(value).unwrap(),
                    _ => unreachable!(),
                }}
                Ok(())
            })?)?;
        }
        let allowed_fields: Vec<TypedReference> = component.parameters.values()
            .filter_map(|value| serde_json::from_value::<TypedReference>(value.clone()).ok())
            .filter(|reference| matches!(reference, TypedReference::Field { .. }))
            .collect();
        for name in ["write_field", "drive_field"] {
            let output = output.clone();
            let allowed = allowed_fields.clone();
            ctx.set(name, lua.create_function(move |lua, (_ctx, reference, value): (Table, LuaValue, LuaValue)| {
                if (name == "drive_field") != local_drive {
                    return Err(error("Use ctx:drive_field only in drive(ctx), and ctx:write_field only in shared callbacks"));
                }
                let reference: TypedReference = lua.from_value(reference)?;
                if !matches!(reference, TypedReference::Field { .. }) || !allowed.contains(&reference) {
                    return Err(error("Target must be a bound field reference"));
                }
                let value: Value = lua.from_value(value)?;
                if serde_json::to_vec(&value).map_or(true, |bytes| bytes.len() > 2048) {
                    return Err(error("Field value is too large"));
                }
                let mut out = output.lock().unwrap();
                out.commands += 1;
                if out.commands > MAX_COMMANDS { return Err(error("Too many script writes")); }
                if local_drive { out.field_drives.push((reference, value)); }
                else { out.field_writes.push((reference, value)); }
                Ok(())
            })?)?;
        }
        let get_output = output.clone();
        ctx.set(
            "get_state",
            lua.create_function(
                move |lua, (_ctx, key, fallback): (Table, String, LuaValue)| {
                    if !identifier(&key) {
                        return Err(error("Invalid state name"));
                    }
                    let output = get_output.lock().unwrap();
                    if let Some(value) = output.state.get(&key) {
                        lua.to_value(value)
                    } else {
                        Ok(fallback)
                    }
                },
            )?,
        )?;
        ctx.set(
            "set_state",
            lua.create_function(move |_, (_ctx, key, value): (Table, String, LuaValue)| {
                if local_drive {
                    return Err(error("Shared state writes are not allowed in drive(ctx)"));
                }
                if !identifier(&key) {
                    return Err(error("Invalid state name"));
                }
                let mut output = output.lock().unwrap();
                output.commands += 1;
                if output.commands > MAX_COMMANDS {
                    return Err(error("Too many script writes"));
                }
                output.state.insert(key, atom(value)?);
                validate_state(&output.state).map_err(error)
            })?,
        )?;
        ctx.set_readonly(true);
        Ok(ctx)
    }
}

pub const ROTATOR: &str = r#"return {
    name = "Rotator",
    fields = {
        speed = { type = "number", default = 45, min = -720, max = 720, label = "Degrees / second" }
    },
    drive = function(ctx)
        for entity in ctx:query("Rotator", "Transform") do
            ctx:drive(entity, "rotation", ctx:rotation_y(entity.Rotator.speed * ctx.time, entity.Transform.rotation))
        end
    end
}"#;

pub const CLICK_COLOR: &str = r#"return {
    name = "ClickColor",
    fields = {
        color = { type = "color", default = {1, 0.3, 0.1, 1}, label = "Click color" }
    },
    on_interact = function(ctx)
        local active = not ctx:get_state("active", false)
        ctx:set_state("active", active)
        ctx:write(ctx.self, "color", active and ctx.self.ClickColor.color or {0.2, 0.65, 0.8, 1})
    end
}"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_defaults_and_invalid_values_are_checked() {
        let p = Program::compile(ROTATOR).unwrap();
        assert_eq!(
            parameters(&p.schema, &BTreeMap::new()).unwrap()["speed"],
            json!(45)
        );
        assert!(parameters(&p.schema, &BTreeMap::from([("speed".into(), json!(900))])).is_err());
        assert!(
            Program::compile("while true do end")
                .err()
                .unwrap()
                .contains("budget")
        );
    }
    #[test]
    fn sandbox_rejects_ambient_io_and_protected_infinite_loops() {
        for key in ["os", "io", "loadstring", "pcall", "coroutine", "getfenv"] {
            assert!(Program::compile(&format!("assert({key} == nil); {CLICK_COLOR}")).is_ok());
        }
        assert!(Program::compile("return {name='Huge',update=function() end,fields={x={type='string',default=string.rep('a',8000000)}}}").is_err());
    }
}
