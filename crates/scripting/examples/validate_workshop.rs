use kokoverse_scripting::{Program, validate_component};
use kokoverse_protocol::ScriptComponent;
use serde_json::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Pass the prepared graphs.json path")?;
    let entries: Vec<Value> = serde_json::from_slice(&std::fs::read(path)?)?;
    for entry in entries {
        if let Some(scripts) = entry["object"]["scripts"].as_array() {
            for script in scripts {
                let script: ScriptComponent = serde_json::from_value(script.clone())?;
                validate_component(&script)?;
                let compiled = Program::compile(&script.source)?;
                if compiled.schema != script.schema {
                    return Err(format!("{} has a stale script schema", script.schema.name).into());
                }
                println!("{}: saved component valid", script.schema.name);
            }
        } else {
            let source = format!("@tukurium-graph-v1\n{entry}");
            let program = Program::compile(&source)?;
            println!("{}: {} fields", program.schema.name, program.schema.fields.len());
        }
    }
    Ok(())
}
