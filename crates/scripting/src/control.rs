//! Validated structured-loop bounds and an invocation-local iteration budget.
use crate::error;
use mlua::{Lua, Table, Value, Variadic};

const MAX_ITERATIONS: i128 = 64;
const MAX_EXACT_INTEGER: f64 = 9_007_199_254_740_991.0;

fn integer(value: &Value) -> mlua::Result<i128> {
    let value = match value {
        Value::Integer(value) => *value as f64,
        Value::Number(value) => *value,
        _ => return Err(error("For Range requires finite integers")),
    };
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > MAX_EXACT_INTEGER {
        return Err(error(
            "For Range requires exact integers within ±(2^53 - 1)",
        ));
    }
    Ok(value as i128)
}

fn count(start: i128, end: i128, step: i128) -> mlua::Result<i128> {
    if step == 0 {
        return Err(error("For Range step must not be zero"));
    }
    if (step > 0 && start > end) || (step < 0 && start < end) {
        return Ok(0);
    }
    let count = (end - start).abs() / step.abs() + 1;
    if count > MAX_ITERATIONS {
        return Err(error(
            "Structured loops allow at most 64 iterations per callback",
        ));
    }
    Ok(count)
}

pub(super) fn install(lua: &Lua, helpers: &Table) -> mlua::Result<()> {
    helpers.set(
        "for_count",
        lua.create_function(|_, (_ctx, args): (Table, Variadic<Value>)| {
            if args.len() != 3 {
                return Err(error("For Range expects start, end and step"));
            }
            Ok(count(integer(&args[0])?, integer(&args[1])?, integer(&args[2])?)? as u32)
        })?,
    )?;
    helpers.set(
        "condition",
        lua.create_function(|_, (_ctx, args): (Table, Variadic<Value>)| {
            if args.len() != 1 {
                return Err(error("Condition expects one boolean"));
            }
            match args[0] {
                Value::Boolean(value) => Ok(value),
                _ => Err(error("Condition must be a boolean")),
            }
        })?,
    )?;
    Ok(())
}

pub(super) fn install_context(lua: &Lua, ctx: &Table) -> mlua::Result<()> {
    let mut iterations = 0;
    ctx.set(
        "loop_step",
        lua.create_function_mut(move |_, (_ctx, args): (Table, Variadic<Value>)| {
            if !args.is_empty() {
                return Err(error("Loop Step expects no arguments"));
            }
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                return Err(error(
                    "Structured loops allow at most 64 iterations per callback",
                ));
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_inclusive_integer_and_bounded() {
        assert_eq!(count(1, 5, 2).unwrap(), 3);
        assert_eq!(count(5, 1, -2).unwrap(), 3);
        assert_eq!(count(5, 1, 1).unwrap(), 0);
        assert_eq!(count(1, 5, -1).unwrap(), 0);
        assert_eq!(count(2, 2, 3).unwrap(), 1);
        assert_eq!(count(0, 63, 1).unwrap(), 64);
        assert!(count(0, 64, 1).is_err());
        assert!(count(0, 1, 0).is_err());
        for value in [
            Value::Number(f64::INFINITY),
            Value::Number(0.5),
            Value::Number(9_007_199_254_740_992.0),
        ] {
            assert!(integer(&value).is_err());
        }
    }

    #[test]
    fn iteration_budget_is_shared_within_one_context_and_reset_for_the_next() {
        let lua = Lua::new();
        for _ in 0..2 {
            let ctx = lua.create_table().unwrap();
            install_context(&lua, &ctx).unwrap();
            let step: mlua::Function = ctx.get("loop_step").unwrap();
            for _ in 0..64 {
                step.call::<()>(ctx.clone()).unwrap();
            }
            assert!(
                step.call::<()>(ctx.clone())
                    .unwrap_err()
                    .to_string()
                    .contains("64 iterations")
            );
        }
    }
}
