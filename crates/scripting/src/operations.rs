//! Bounded pure operations shared by scripts and visual graph nodes.
use crate::{
    error,
    math::{finite, length, number},
};
use mlua::{Lua, LuaSerdeExt, Table, Value, Variadic};

fn boolean(value: &Value) -> mlua::Result<bool> {
    match value {
        Value::Boolean(value) => Ok(*value),
        _ => Err(error("Expected a boolean")),
    }
}

fn text(value: &Value) -> mlua::Result<String> {
    let Value::String(value) = value else {
        return Err(error("Expected a string"));
    };
    let value = value.to_str()?;
    if value.len() > 512 {
        return Err(error("Text must contain at most 512 UTF-8 bytes"));
    }
    Ok(value.to_string())
}

fn array<const N: usize>(value: &Value) -> mlua::Result<[f64; N]> {
    let Value::Table(table) = value else {
        return Err(error(format!("Expected exactly {N} finite numbers")));
    };
    if table.raw_len() != N {
        return Err(error(format!("Expected exactly {N} finite numbers")));
    }
    let mut result = [0.0; N];
    let mut seen = [false; N];
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let index = match key {
            Value::Integer(index) if index >= 1 && index as usize <= N => index as usize - 1,
            Value::Number(index) if (1.0..=N as f64).contains(&index) && index.fract() == 0.0 => {
                index as usize - 1
            }
            _ => return Err(error(format!("Expected exactly {N} finite numbers"))),
        };
        result[index] = number(&value)?;
        seen[index] = true;
    }
    if seen.into_iter().all(|present| present) {
        Ok(result)
    } else {
        Err(error(format!("Expected exactly {N} finite numbers")))
    }
}

fn color(value: &Value) -> mlua::Result<[f64; 4]> {
    let result = array(value)?;
    if result.iter().all(|value| (0.0..=1.0).contains(value)) {
        Ok(result)
    } else {
        Err(error("Color channels must be between zero and one"))
    }
}

fn atom(value: &Value) -> mlua::Result<serde_json::Value> {
    let result = match value {
        Value::Nil => return Ok(serde_json::Value::Null),
        Value::Boolean(value) => serde_json::json!(value),
        Value::Integer(_) | Value::Number(_) => serde_json::json!(number(value)?),
        Value::String(_) => serde_json::json!(text(value)?),
        Value::Table(table) => match table.raw_len() {
            2 => serde_json::json!(array::<2>(value)?),
            3 => serde_json::json!(array::<3>(value)?),
            4 => serde_json::json!(array::<4>(value)?),
            _ => return Err(error("Expected a small JSON atom")),
        },
        _ => return Err(error("Expected a small JSON atom")),
    };
    Ok(result)
}

fn normalized(value: [f64; 3]) -> mlua::Result<[f64; 3]> {
    let scale = value
        .iter()
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    if scale == 0.0 {
        return Err(error("A zero vector has no direction"));
    }
    let scaled = value.map(|value| value / scale);
    let magnitude = length(scaled);
    Ok(scaled.map(|value| value / magnitude))
}

fn blend(a: f64, b: f64, t: f64) -> mlua::Result<f64> {
    finite(if t == 0.0 {
        a
    } else if t == 1.0 {
        b
    } else {
        (1.0 - t) * a + t * b
    })
}

fn fraction(value: f64, min: f64, max: f64) -> mlua::Result<f64> {
    if value == min {
        return Ok(0.0);
    }
    if value == max {
        return Ok(1.0);
    }
    let numerator = value - min;
    let denominator = max - min;
    if numerator.is_finite() && denominator.is_finite() {
        finite(numerator / denominator)
    } else {
        let scale = value.abs().max(min.abs()).max(max.abs());
        finite((value / scale - min / scale) / (max / scale - min / scale))
    }
}

fn string_index(value: &Value, limit: usize) -> mlua::Result<usize> {
    let value = number(value)?;
    if value < 0.0 || value.fract() != 0.0 {
        return Err(error("String indices must be nonnegative integers"));
    }
    Ok(value.min(limit as f64) as usize)
}

fn arity(name: &str) -> usize {
    match name {
        "remap" => 5,
        "color_make" => 4,
        "smoothstep" | "string_slice" | "vec3_move_towards" | "color_lerp" => 3,
        "power" | "atan2" | "ping_pong" | "less_equal" | "greater_equal" | "equal"
        | "not_equal" | "xor" | "string_contains" | "string_starts_with" | "string_ends_with"
        | "vec3_angle" | "vec3_reflect" => 2,
        _ => 1,
    }
}

fn evaluate(lua: &Lua, name: &str, args: &[Value]) -> mlua::Result<Value> {
    if args.len() != arity(name) {
        return Err(error(format!("{name} expects {} arguments", arity(name))));
    }
    let value = match name {
        "less_equal" => return Ok(Value::Boolean(number(&args[0])? <= number(&args[1])?)),
        "greater_equal" => return Ok(Value::Boolean(number(&args[0])? >= number(&args[1])?)),
        "equal" | "not_equal" => {
            let equal = atom(&args[0])? == atom(&args[1])?;
            return Ok(Value::Boolean(if name == "equal" { equal } else { !equal }));
        }
        "xor" => return Ok(Value::Boolean(boolean(&args[0])? ^ boolean(&args[1])?)),
        "number_to_bool" => return Ok(Value::Boolean(number(&args[0])? != 0.0)),
        "bool_to_number" => f64::from(u8::from(boolean(&args[0])?)),
        "parse_number" => text(&args[0])?
            .trim()
            .parse::<f64>()
            .map_err(|_| error("Expected a finite decimal number"))?,
        "string_length" => text(&args[0])?.chars().count() as f64,
        "string_contains" | "string_starts_with" | "string_ends_with" => {
            let value = text(&args[0])?;
            let part = text(&args[1])?;
            return Ok(Value::Boolean(match name {
                "string_contains" => value.contains(&part),
                "string_starts_with" => value.starts_with(&part),
                _ => value.ends_with(&part),
            }));
        }
        "string_slice" => {
            let value = text(&args[0])?;
            let limit = value.chars().count();
            let start = string_index(&args[1], limit)?;
            let count = string_index(&args[2], limit)?;
            let result: String = value.chars().skip(start).take(count).collect();
            return Ok(Value::String(lua.create_string(result)?));
        }
        "vec3_angle" => {
            let a = normalized(array(&args[0])?)?;
            let b = normalized(array(&args[1])?)?;
            let cross = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            length(cross).atan2(a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
        }
        "vec3_reflect" => {
            let value: [f64; 3] = array(&args[0])?;
            let normal = normalized(array(&args[1])?)?;
            let scale = value
                .iter()
                .map(|value| value.abs())
                .fold(0.0_f64, f64::max);
            let result = if scale == 0.0 {
                [0.0; 3]
            } else {
                let scaled = value.map(|value| value / scale);
                let dot = scaled[0] * normal[0] + scaled[1] * normal[1] + scaled[2] * normal[2];
                std::array::from_fn(|i| (scaled[i] - 2.0 * dot * normal[i]) * scale)
            };
            for value in result {
                finite(value)?;
            }
            return lua.to_value(&result);
        }
        "vec3_move_towards" => {
            let a: [f64; 3] = array(&args[0])?;
            let b: [f64; 3] = array(&args[1])?;
            let max_delta = number(&args[2])?;
            if max_delta < 0.0 {
                return Err(error("Move distance must be nonnegative"));
            }
            let result = if a == b {
                b
            } else if max_delta == 0.0 {
                a
            } else {
                let difference: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
                let (scale, scaled) = if difference.iter().all(|value| value.is_finite()) {
                    let scale = difference
                        .iter()
                        .map(|value| value.abs())
                        .fold(0.0_f64, f64::max);
                    (scale, difference.map(|value| value / scale))
                } else {
                    let scale = a
                        .iter()
                        .chain(&b)
                        .map(|value| value.abs())
                        .fold(0.0_f64, f64::max);
                    (scale, std::array::from_fn(|i| b[i] / scale - a[i] / scale))
                };
                let distance = length(difference);
                let t = if distance.is_finite() && max_delta >= distance {
                    1.0
                } else {
                    ((max_delta / scale) / length(scaled)).min(1.0)
                };
                [
                    blend(a[0], b[0], t)?,
                    blend(a[1], b[1], t)?,
                    blend(a[2], b[2], t)?,
                ]
            };
            return lua.to_value(&result);
        }
        "color_make" => {
            let result = [
                number(&args[0])?,
                number(&args[1])?,
                number(&args[2])?,
                number(&args[3])?,
            ];
            if !result.iter().all(|value| (0.0..=1.0).contains(value)) {
                return Err(error("Color channels must be between zero and one"));
            }
            return lua.to_value(&result);
        }
        "color_r" | "color_g" | "color_b" | "color_a" => {
            let index = match name {
                "color_r" => 0,
                "color_g" => 1,
                "color_b" => 2,
                _ => 3,
            };
            color(&args[0])?[index]
        }
        "color_lerp" => {
            let a = color(&args[0])?;
            let b = color(&args[1])?;
            let t = number(&args[2])?.clamp(0.0, 1.0);
            let result = [
                blend(a[0], b[0], t)?,
                blend(a[1], b[1], t)?,
                blend(a[2], b[2], t)?,
                blend(a[3], b[3], t)?,
            ];
            return lua.to_value(&result);
        }
        "power" => number(&args[0])?.powf(number(&args[1])?),
        "atan2" => number(&args[0])?.atan2(number(&args[1])?),
        "remap" => {
            let value = number(&args[0])?;
            let min = number(&args[1])?;
            let max = number(&args[2])?;
            let out_min = number(&args[3])?;
            let out_max = number(&args[4])?;
            if min == max {
                return Err(error("Remap input endpoints must differ"));
            }
            if out_min == out_max {
                out_min
            } else {
                let t = fraction(value, min, max)?;
                if t == 0.0 {
                    return Ok(Value::Number(out_min));
                }
                if t == 1.0 {
                    return Ok(Value::Number(out_max));
                }
                let difference = out_max - out_min;
                let mapped = out_min + t * difference;
                if difference.is_finite() && mapped.is_finite() {
                    mapped
                } else {
                    blend(out_min, out_max, t)?
                }
            }
        }
        "smoothstep" => {
            let min = number(&args[0])?;
            let max = number(&args[1])?;
            let value = number(&args[2])?;
            if min >= max {
                return Err(error("Smoothstep edges must be increasing"));
            }
            let t = if value <= min {
                0.0
            } else if value >= max {
                1.0
            } else {
                fraction(value, min, max)?.clamp(0.0, 1.0)
            };
            t * t * (3.0 - 2.0 * t)
        }
        "ping_pong" => {
            let value = number(&args[0])?.abs();
            let length = number(&args[1])?;
            if length <= 0.0 {
                return Err(error("Ping-pong length must be positive"));
            }
            let phase = if length > f64::MAX / 2.0 {
                value
            } else {
                value.rem_euclid(2.0 * length)
            };
            (if phase <= length {
                phase
            } else {
                length - (phase - length)
            })
            .clamp(0.0, length)
        }
        _ => {
            let value = number(&args[0])?;
            match name {
                "negate" => -value,
                "sqrt" => value.sqrt(),
                "round" => value.round(),
                "sign" => {
                    if value == 0.0 {
                        0.0
                    } else {
                        value.signum()
                    }
                }
                "tan" => value.tan(),
                "asin" => value.asin(),
                "acos" => value.acos(),
                "log" => value.ln(),
                "exp" => value.exp(),
                _ => unreachable!(),
            }
        }
    };
    Ok(Value::Number(finite(value)?))
}

pub(super) fn install(lua: &Lua, ctx: &Table) -> mlua::Result<()> {
    for name in [
        "negate",
        "sqrt",
        "power",
        "round",
        "sign",
        "tan",
        "asin",
        "acos",
        "atan2",
        "log",
        "exp",
        "remap",
        "smoothstep",
        "ping_pong",
        "less_equal",
        "greater_equal",
        "equal",
        "not_equal",
        "xor",
        "parse_number",
        "number_to_bool",
        "bool_to_number",
        "string_length",
        "string_contains",
        "string_starts_with",
        "string_ends_with",
        "string_slice",
        "vec3_angle",
        "vec3_reflect",
        "vec3_move_towards",
        "color_make",
        "color_r",
        "color_g",
        "color_b",
        "color_a",
        "color_lerp",
    ] {
        ctx.set(
            name,
            lua.create_function(move |lua, (_ctx, args): (Table, Variadic<Value>)| {
                evaluate(lua, name, &args)
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn eval(source: &str) -> mlua::Result<serde_json::Value> {
        let lua = Lua::new();
        let ctx = lua.create_table()?;
        install(&lua, &ctx)?;
        lua.globals().set("ctx", ctx)?;
        let mut value = lua.from_value(lua.load(source).eval()?)?;
        // Luau stores integral doubles as Integer values when crossing into Rust.
        fn normalize_numbers(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Number(number) => *value = json!(number.as_f64().unwrap()),
                serde_json::Value::Array(values) => values.iter_mut().for_each(normalize_numbers),
                _ => {}
            }
        }
        normalize_numbers(&mut value);
        Ok(value)
    }

    #[test]
    fn scalar_operations_and_domains_are_strict() {
        for (expression, expected) in [
            ("ctx:negate(-4)", 4.0),
            ("ctx:sqrt(9)", 3.0),
            ("ctx:power(2,3)", 8.0),
            ("ctx:round(-1.5)", -2.0),
            ("ctx:round(1.5)", 2.0),
            ("ctx:sign(-0.0)", 0.0),
            ("ctx:sign(-0.5)", -1.0),
            ("ctx:tan(0)", 0.0),
            ("ctx:asin(0)", 0.0),
            ("ctx:acos(1)", 0.0),
            ("ctx:atan2(0,1)", 0.0),
            ("ctx:log(1)", 0.0),
            ("ctx:exp(0)", 1.0),
        ] {
            assert_eq!(
                eval(&format!("return {expression}")).unwrap(),
                json!(expected)
            );
        }
        for name in [
            "negate", "sqrt", "round", "sign", "tan", "asin", "acos", "log", "exp",
        ] {
            for value in ["'1'", "false", "nil", "0/0", "math.huge"] {
                assert!(
                    eval(&format!("return ctx:{name}({value})")).is_err(),
                    "{name} accepted {value}"
                );
            }
        }
        for expression in [
            "ctx:sqrt(-1)",
            "ctx:power(-1,0.5)",
            "ctx:power(1e308,2)",
            "ctx:asin(2)",
            "ctx:acos(-2)",
            "ctx:log(0)",
            "ctx:log(-1)",
            "ctx:exp(1000)",
            "ctx:atan2(1,'2')",
            "ctx:power(2)",
            "ctx:sign(1,2)",
        ] {
            assert!(
                eval(&format!("return {expression}")).is_err(),
                "accepted {expression}"
            );
        }
    }

    #[test]
    fn mapping_functions_handle_edges_and_negative_phases() {
        for (expression, expected) in [
            ("ctx:remap(15,0,10,0,100)", 150.0),
            ("ctx:remap(5,10,0,0,100)", 50.0),
            ("ctx:remap(0,-1e308,1e308,-1e308,1e308)", 0.0),
            ("ctx:remap(1,0,1,-1e308,1e-308)", 1e-308),
            ("ctx:smoothstep(0,2,1)", 0.5),
            ("ctx:smoothstep(0,2,-1)", 0.0),
            ("ctx:smoothstep(-1e308,1e308,0)", 0.5),
            ("ctx:smoothstep(0,2,3)", 1.0),
            ("ctx:ping_pong(-1,3)", 1.0),
            ("ctx:ping_pong(-4,3)", 2.0),
            ("ctx:ping_pong(6,3)", 0.0),
            ("ctx:ping_pong(1e308,1e308)", 1e308),
        ] {
            assert_eq!(
                eval(&format!("return {expression}")).unwrap(),
                json!(expected)
            );
        }
        for expression in [
            "ctx:remap(0,1,1,0,2)",
            "ctx:smoothstep(2,1,0)",
            "ctx:smoothstep(1,1,0)",
            "ctx:ping_pong(1,0)",
            "ctx:ping_pong(1,-1)",
        ] {
            assert!(eval(&format!("return {expression}")).is_err());
        }
    }

    #[test]
    fn comparisons_and_conversions_preserve_false_values() {
        for (expression, expected) in [
            ("ctx:less_equal(2,2)", json!(true)),
            ("ctx:greater_equal(-1,0)", json!(false)),
            ("ctx:not_equal(false,false)", json!(false)),
            ("ctx:not_equal(false,0)", json!(true)),
            ("ctx:equal(nil,nil)", json!(true)),
            ("ctx:not_equal(nil,false)", json!(true)),
            ("ctx:equal({1,2},{1,2,3})", json!(false)),
            ("ctx:equal(3,3.0)", json!(true)),
            ("ctx:equal(1e18,1e18)", json!(true)),
            ("ctx:equal({1e308,2},{1e308,2})", json!(true)),
            ("ctx:not_equal(1,1.0)", json!(false)),
            ("ctx:not_equal({1,2,3},{1,2,3})", json!(false)),
            ("ctx:not_equal('x','y')", json!(true)),
            ("ctx:xor(false,true)", json!(true)),
            ("ctx:xor(true,true)", json!(false)),
            ("ctx:number_to_bool(-0.0)", json!(false)),
            ("ctx:number_to_bool(-3)", json!(true)),
            ("ctx:bool_to_number(false)", json!(0.0)),
            ("ctx:bool_to_number(true)", json!(1.0)),
            ("ctx:parse_number('  -1.25e2  ')", json!(-125.0)),
        ] {
            assert_eq!(eval(&format!("return {expression}")).unwrap(), expected);
        }
        for expression in [
            "ctx:not_equal({1,2,extra=3},{1,2})",
            "ctx:not_equal({1,'2'},{1,2})",
            "ctx:equal({}, {})",
            "ctx:equal(function() end,0)",
            "ctx:equal(0/0,0)",
            "ctx:less_equal('1',2)",
            "ctx:xor(0,false)",
            "ctx:number_to_bool('0')",
            "ctx:bool_to_number(1)",
            "ctx:parse_number(12)",
            "ctx:parse_number('NaN')",
            "ctx:parse_number('inf')",
            "ctx:parse_number('1e309')",
            "ctx:parse_number('0x10')",
            "ctx:parse_number('1_000')",
            "ctx:parse_number('')",
        ] {
            assert!(
                eval(&format!("return {expression}")).is_err(),
                "accepted {expression}"
            );
        }
    }

    #[test]
    fn strings_use_unicode_characters_and_bounded_utf8() {
        for (expression, expected) in [
            ("ctx:string_length('가나漢字🙂')", json!(5.0)),
            ("ctx:string_contains('가나漢字🙂','漢字')", json!(true)),
            (
                "ctx:string_starts_with('한국어日本語🙂','한국')",
                json!(true),
            ),
            ("ctx:string_ends_with('한국어日本語🙂','🙂')", json!(true)),
            ("ctx:string_slice('가나漢字🙂',1,3)", json!("나漢字")),
            ("ctx:string_slice('가나漢字🙂',4,100)", json!("🙂")),
            ("ctx:string_slice('가나漢字🙂',100,2)", json!("")),
            ("ctx:string_slice('가나漢字🙂',0,0)", json!("")),
        ] {
            assert_eq!(eval(&format!("return {expression}")).unwrap(), expected);
        }
        for expression in [
            "ctx:string_slice('abc',-1,1)",
            "ctx:string_slice('abc',0,0.5)",
            "ctx:string_slice('abc','0',1)",
            "ctx:string_length(2)",
            "ctx:string_contains('abc',false)",
            "ctx:string_length(string.rep('a',513))",
            "ctx:string_slice(string.rep('🙂',129),0,1)",
            "ctx:string_length(string.char(255))",
        ] {
            assert!(
                eval(&format!("return {expression}")).is_err(),
                "accepted {expression}"
            );
        }
        assert_eq!(
            eval("return ctx:string_length(string.rep('🙂',128))").unwrap(),
            json!(128.0)
        );
    }

    #[test]
    fn vector_geometry_normalizes_normals_and_stops_at_target() {
        let angle = eval("return ctx:vec3_angle({1e308,0,0},{0,1e-308,0})")
            .unwrap()
            .as_f64()
            .unwrap();
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(
            eval("return ctx:vec3_reflect({1,-1,0},{0,5,0})").unwrap(),
            json!([1.0, 1.0, 0.0])
        );
        assert_eq!(
            eval("return ctx:vec3_reflect({0,0,0},{0,1,0})").unwrap(),
            json!([0.0, 0.0, 0.0])
        );
        assert_eq!(
            eval("return ctx:vec3_move_towards({0,0,0},{3,4,0},10)").unwrap(),
            json!([3.0, 4.0, 0.0])
        );
        let halfway = eval("return ctx:vec3_move_towards({0,0,0},{3,4,0},2.5)").unwrap();
        assert_eq!(halfway, json!([1.5, 2.0, 0.0]));
        assert_eq!(
            eval("return ctx:vec3_move_towards({-1e308,0,0},{1e308,0,0},1e308)").unwrap(),
            json!([0.0, 0.0, 0.0])
        );
        for expression in [
            "ctx:vec3_angle({0,0,0},{1,0,0})",
            "ctx:vec3_reflect({1,2,3},{0,0,0})",
            "ctx:vec3_move_towards({0,0,0},{1,0,0},-1)",
            "ctx:vec3_angle({1,2},{1,2,3})",
            "ctx:vec3_angle({1,2,3,extra=1},{1,2,3})",
            "ctx:vec3_reflect({1,2,'3'},{0,1,0})",
            "ctx:vec3_move_towards({0,0,0},{math.huge,0,0},1)",
        ] {
            assert!(
                eval(&format!("return {expression}")).is_err(),
                "accepted {expression}"
            );
        }
    }

    #[test]
    fn colors_validate_rgba_and_clamp_interpolation_only() {
        assert_eq!(
            eval("return ctx:color_make(0.2,0.4,0.6,1)").unwrap(),
            json!([0.2, 0.4, 0.6, 1.0])
        );
        for (name, expected) in [
            ("color_r", 0.2),
            ("color_g", 0.4),
            ("color_b", 0.6),
            ("color_a", 1.0),
        ] {
            assert_eq!(
                eval(&format!("return ctx:{name}({{0.2,0.4,0.6,1}})")).unwrap(),
                json!(expected)
            );
        }
        assert_eq!(
            eval("return ctx:color_lerp({0,0,0,0},{1,1,1,1},0.25)").unwrap(),
            json!([0.25, 0.25, 0.25, 0.25])
        );
        assert_eq!(
            eval("return ctx:color_lerp({0.2,0.4,0.6,1},{1,1,1,0},-1)").unwrap(),
            json!([0.2, 0.4, 0.6, 1.0])
        );
        assert_eq!(
            eval("return ctx:color_lerp({0,0,0,0},{1,1,1,1},2)").unwrap(),
            json!([1.0, 1.0, 1.0, 1.0])
        );
        for expression in [
            "ctx:color_make(-0.1,0,0,1)",
            "ctx:color_make(0,0,0,1.1)",
            "ctx:color_make('0',0,0,1)",
            "ctx:color_r({0,0,0})",
            "ctx:color_a({0,0,0,1,extra=1})",
            "ctx:color_lerp({0,0,0,0},{2,0,0,1},0)",
            "ctx:color_lerp({0,0,0,0},{1,1,1,1},0/0)",
        ] {
            assert!(
                eval(&format!("return {expression}")).is_err(),
                "accepted {expression}"
            );
        }
    }
}
