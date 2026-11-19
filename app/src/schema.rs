//! Faza B, pkt 5: walidacja argumentów względem schematu z `tools/list`.
//!
//! Minimalna, zależna tylko od `serde_json` (bez nowych crate'ów): sprawdza typ obiektu,
//! wymagane pola, typy deklarowanych pól, `maxLength` pól tekstowych i
//! `additionalProperties: false`. To pokrywa schematy z [`crate::tools::input_schema`];
//! głębsze konstrukcje (oneOf, patterny) nie występują w naszych narzędziach, a gdy się
//! pojawią, walidator ma je przepuszczać, nie blokować — dlatego nieznane słowa
//! kluczowe są ignorowane.

use serde_json::Value;

/// F25: górna granica pola tekstowego (message, channel). Bez niej 3 MB komunikat
/// trafiał w całości do logu outboxa. Liczona w bajtach — tyle zajmuje tekst na dysku.
pub const MAX_TEXT_FIELD_BYTES: usize = 65_536;

/// Zwraca `Ok(())`, gdy `args` pasują do schematu `schema`, albo `Err(opis)` po polsku.
pub fn validate(schema: &Value, args: &Value) -> Result<(), String> {
    let obj = match args.as_object() {
        Some(o) => o,
        None => return Err("argumenty musza byc obiektem JSON".to_string()),
    };

    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for key in required.iter().filter_map(|k| k.as_str()) {
            if !obj.contains_key(key) {
                return Err(format!("brak wymaganego argumentu '{}'", key));
            }
        }
    }

    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (key, value) in obj.iter() {
            match props.get(key) {
                Some(prop) => {
                    if let Some(expected) = prop.get("type").and_then(|t| t.as_str()) {
                        if !type_matches(expected, value) {
                            return Err(format!(
                                "argument '{}' ma zly typ: oczekiwano {}, jest {}",
                                key,
                                expected,
                                json_type(value)
                            ));
                        }
                    }
                    if let Some(limit) = prop.get("maxLength").and_then(|v| v.as_u64()) {
                        if let Some(text) = value.as_str() {
                            let bytes = text.len() as u64;
                            if bytes > limit {
                                return Err(format!(
                                    "argument '{}' ma {} bajtow, limit to {}",
                                    key, bytes, limit
                                ));
                            }
                        }
                    }
                }
                None => {
                    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                        return Err(format!("argument '{}' nie jest dozwolony", key));
                    }
                }
            }
        }
    }

    Ok(())
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true, // nieznany typ: nie blokuj
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_schema() -> Value {
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"],
            "additionalProperties": false
        })
    }

    #[test]
    fn accepts_valid_args() {
        assert!(validate(&read_schema(), &json!({ "path": "a.txt" })).is_ok());
    }

    #[test]
    fn rejects_missing_required() {
        let err = validate(&read_schema(), &json!({})).unwrap_err();
        assert!(err.contains("brak wymaganego argumentu 'path'"), "{err}");
    }

    #[test]
    fn rejects_wrong_type() {
        let err = validate(&read_schema(), &json!({ "path": 1 })).unwrap_err();
        assert!(err.contains("zly typ"), "{err}");
    }

    #[test]
    fn rejects_unknown_property() {
        let err = validate(&read_schema(), &json!({ "path": "a", "extra": 1 })).unwrap_err();
        assert!(err.contains("nie jest dozwolony"), "{err}");
    }

    #[test]
    fn rejects_non_object() {
        assert!(validate(&read_schema(), &json!([1, 2])).is_err());
        assert!(validate(&read_schema(), &json!("x")).is_err());
    }

    /// F25: `maxLength` ze schematu jest egzekwowane — w bajtach, bo tyle
    /// zajmuje tekst na dysku.
    #[test]
    fn enforces_max_length() {
        let schema = json!({
            "type": "object",
            "properties": { "message": { "type": "string", "maxLength": 4 } },
            "required": ["message"],
            "additionalProperties": false
        });
        assert!(validate(&schema, &json!({ "message": "abcd" })).is_ok());
        let err = validate(&schema, &json!({ "message": "abcde" })).unwrap_err();
        assert!(err.contains("limit"), "{err}");
        assert!(validate(&schema, &json!({ "message": "za dlugie" })).is_err());
    }
}
