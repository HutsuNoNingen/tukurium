//! Pure numeric helpers shared by Luau scripts and compiled visual graphs.
use crate::error;
use mlua::{Lua, LuaSerdeExt, Table, Value, Variadic};

pub(super) fn number(value: &Value) -> mlua::Result<f64> {
    let result = match value {
        Value::Integer(n) => *n as f64,
        Value::Number(n) => *n,
        _ => return Err(error("Expected a finite number")),
    };
    finite(result)
}

pub(super) fn finite(value: f64) -> mlua::Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(error("Math input and result must be finite"))
    }
}

pub(super) fn vector(value: &Value) -> mlua::Result<[f64; 3]> {
    let Value::Table(table) = value else {
        return Err(error("Expected exactly three finite numbers"));
    };
    let mut result = [0.0; 3];
    let mut seen = [false; 3];
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let index = match key {
            Value::Integer(index @ 1..=3) => index as usize - 1,
            Value::Number(index) if [1.0, 2.0, 3.0].contains(&index) => index as usize - 1,
            _ => return Err(error("Expected exactly three finite numbers")),
        };
        result[index] = number(&value)?;
        seen[index] = true;
    }
    if seen.iter().all(|present| *present) {
        Ok(result)
    } else {
        Err(error("Expected exactly three finite numbers"))
    }
}

pub(super) fn lerp(a: f64, b: f64, t: f64) -> mlua::Result<f64> {
    let t = t.clamp(0.0, 1.0);
    // Preserve exact endpoints and avoid overflowing b - a for opposite extremes.
    finite(if t == 0.0 {
        a
    } else if t == 1.0 {
        b
    } else {
        (1.0 - t) * a + t * b
    })
}

pub(super) fn length(value: [f64; 3]) -> f64 {
    value[0].hypot(value[1]).hypot(value[2])
}

pub(super) fn install(lua: &Lua, ctx: &Table) -> mlua::Result<()> {
    for name in [
        "abs",
        "floor",
        "sin",
        "cos",
        "clamp",
        "lerp",
        "vec3_make",
        "vec3_x",
        "vec3_y",
        "vec3_z",
        "vec3_add",
        "vec3_subtract",
        "vec3_scale",
        "vec3_dot",
        "vec3_cross",
        "vec3_length",
        "vec3_normalize",
        "vec3_distance",
        "vec3_lerp",
    ] {
        ctx.set(
            name,
            lua.create_function(move |lua, (_ctx, args): (Table, Variadic<Value>)| {
                let arity = match name {
                    "clamp" | "lerp" | "vec3_make" | "vec3_lerp" => 3,
                    "vec3_add" | "vec3_subtract" | "vec3_scale" | "vec3_dot" | "vec3_cross"
                    | "vec3_distance" => 2,
                    _ => 1,
                };
                if args.len() != arity {
                    return Err(error(format!("{name} expects {arity} arguments")));
                }
                let scalar = match name {
                    "abs" | "floor" | "sin" | "cos" => {
                        let value = number(&args[0])?;
                        Some(match name {
                            "abs" => value.abs(),
                            "floor" => value.floor(),
                            "sin" => value.sin(),
                            _ => value.cos(),
                        })
                    }
                    "clamp" => {
                        let value = number(&args[0])?;
                        let min = number(&args[1])?;
                        let max = number(&args[2])?;
                        if min > max {
                            return Err(error("Clamp minimum must not exceed maximum"));
                        }
                        Some(value.clamp(min, max))
                    }
                    "lerp" => Some(lerp(
                        number(&args[0])?,
                        number(&args[1])?,
                        number(&args[2])?,
                    )?),
                    "vec3_x" | "vec3_y" | "vec3_z" => {
                        let index = match name {
                            "vec3_x" => 0,
                            "vec3_y" => 1,
                            _ => 2,
                        };
                        Some(vector(&args[0])?[index])
                    }
                    "vec3_length" => Some(length(vector(&args[0])?)),
                    "vec3_dot" | "vec3_distance" => {
                        let a = vector(&args[0])?;
                        let b = vector(&args[1])?;
                        Some(if name == "vec3_dot" {
                            a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
                        } else {
                            length(std::array::from_fn(|i| a[i] - b[i]))
                        })
                    }
                    _ => None,
                };
                if let Some(value) = scalar {
                    return Ok(Value::Number(finite(value)?));
                }
                let result = match name {
                    "vec3_make" => [number(&args[0])?, number(&args[1])?, number(&args[2])?],
                    "vec3_scale" => {
                        let value = vector(&args[0])?;
                        let factor = number(&args[1])?;
                        value.map(|n| n * factor)
                    }
                    "vec3_normalize" => {
                        let value = vector(&args[0])?;
                        let max = value.iter().map(|n| n.abs()).fold(0.0_f64, f64::max);
                        if max == 0.0 {
                            [0.0; 3]
                        } else {
                            let scaled = value.map(|n| n / max);
                            let length = length(scaled);
                            scaled.map(|n| n / length)
                        }
                    }
                    "vec3_add" | "vec3_subtract" | "vec3_cross" | "vec3_lerp" => {
                        let a = vector(&args[0])?;
                        let b = vector(&args[1])?;
                        match name {
                            "vec3_add" => std::array::from_fn(|i| a[i] + b[i]),
                            "vec3_subtract" => std::array::from_fn(|i| a[i] - b[i]),
                            "vec3_cross" => [
                                a[1] * b[2] - a[2] * b[1],
                                a[2] * b[0] - a[0] * b[2],
                                a[0] * b[1] - a[1] * b[0],
                            ],
                            _ => {
                                let t = number(&args[2])?;
                                [
                                    lerp(a[0], b[0], t)?,
                                    lerp(a[1], b[1], t)?,
                                    lerp(a[2], b[2], t)?,
                                ]
                            }
                        }
                    }
                    _ => unreachable!(),
                };
                for value in result {
                    finite(value)?;
                }
                lua.to_value(&result)
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str) -> mlua::Result<serde_json::Value> {
        let lua = Lua::new();
        let ctx = lua.create_table()?;
        install(&lua, &ctx)?;
        lua.globals().set("ctx", ctx)?;
        lua.from_value(lua.load(source).eval()?)
    }

    #[test]
    fn interpolation_clamps_t_and_preserves_extreme_endpoints() {
        assert_eq!(
            eval("return ctx:lerp(-1e308,1e308,0)").unwrap(),
            serde_json::json!(-1e308)
        );
        assert_eq!(
            eval("return ctx:lerp(-1e308,1e308,1)").unwrap(),
            serde_json::json!(1e308)
        );
        assert_eq!(
            eval("return ctx:lerp(-1e308,1e308,0.5)").unwrap(),
            serde_json::json!(0)
        );
        assert_eq!(
            eval("return ctx:vec3_lerp({1,2,3},{4,5,6},-2)").unwrap(),
            serde_json::json!([1, 2, 3])
        );
        assert_eq!(
            eval("return ctx:vec3_lerp({1,2,3},{4,5,6},2)").unwrap(),
            serde_json::json!([4, 5, 6])
        );
        assert_eq!(
            eval("return ctx:vec3_normalize({1e308,0,0})").unwrap(),
            serde_json::json!([1, 0, 0])
        );
        assert_eq!(
            eval("return ctx:vec3_normalize({1e-308,0,0})").unwrap(),
            serde_json::json!([1, 0, 0])
        );
        assert_eq!(
            eval("return ctx:clamp(2,2,2)").unwrap(),
            serde_json::json!(2)
        );
    }

    #[test]
    fn helpers_reject_malformed_nonfinite_and_overflowing_values() {
        for name in ["abs", "floor", "sin", "cos"] {
            for value in ["'3'", "0/0", "math.huge", "false", "nil"] {
                let source = format!("return ctx:{name}({value})");
                assert!(eval(&source).is_err(), "accepted {source}");
            }
        }
        for source in [
            "return ctx:vec3_make('1',2,3)",
            "return ctx:vec3_make(1,2)",
            "return ctx:vec3_make(1,2,3,4)",
            "return ctx:vec3_length({1,2})",
            "return ctx:vec3_length({1,2,3,4})",
            "return ctx:vec3_length({1,nil,3})",
            "return ctx:vec3_length({1,2,3,extra=4})",
            "return ctx:vec3_length({1,'2',3})",
            "return ctx:vec3_length({1,false,3})",
            "return ctx:vec3_scale({1,2,3},0/0)",
            "return ctx:vec3_add({1,2,3},{1,2,math.huge})",
            "return ctx:vec3_scale({1e308,0,0},1e308)",
            "return ctx:vec3_dot({1e308,0,0},{1e308,0,0})",
            "return ctx:vec3_cross({1e308,0,0},{0,1e308,0})",
            "return ctx:vec3_length({1.7e308,1.7e308,1.7e308})",
            "return ctx:vec3_distance({1e308,0,0},{-1e308,0,0})",
            "return ctx:clamp(0,2,1)",
            "return ctx:clamp(0,-math.huge,1)",
            "return ctx:lerp(1,2,math.huge)",
            "return ctx:vec3_lerp({1,2,3},{4,5,6},0/0)",
            "return ctx:vec3_x({1,2,3,4})",
        ] {
            assert!(eval(source).is_err(), "accepted {source}");
        }
    }
}
