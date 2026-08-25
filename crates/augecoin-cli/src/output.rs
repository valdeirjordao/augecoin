use serde_json::Value;

pub fn print_output(value: &Value, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    } else {
        print_tabular(value);
    }
}

fn print_tabular(value: &Value) {
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                println!("(empty)");
                return;
            }
            print_array_table(items);
        }
        Value::Object(map) => {
            if let Some(validators) = map.get("validators") {
                if let Some(items) = validators.as_array() {
                    if items.is_empty() {
                        println!("(no validators)");
                        return;
                    }
                    print_array_table(items);
                    return;
                }
            }
            print_object_table(map);
        }
        _ => {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }
    }
}

fn print_array_table(items: &[Value]) {
    if items.is_empty() {
        println!("(no items)");
        return;
    }
    let keys = collect_keys(items);
    if keys.is_empty() {
        for item in items {
            println!("{}", serde_json::to_string(item).unwrap_or_default());
        }
        return;
    }

    let headers: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let mut rows: Vec<Vec<String>> = Vec::new();

    for item in items {
        let mut row = Vec::new();
        for key in &keys {
            let val = item.get(key).map(format_value).unwrap_or_default();
            row.push(val);
        }
        rows.push(row);
    }

    print_table(&headers, &rows);
}

fn print_object_table(obj: &serde_json::Map<String, Value>) {
    let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
    keys.sort();
    let rows = vec![keys.iter().map(|k| format_value(&obj[*k])).collect()];
    print_table(&keys, &rows);
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.len());
            }
        }
    }

    print_separator(&col_widths, "\u{250c}", "\u{252c}", "\u{2510}");

    print!("\u{2502}");
    for (i, header) in headers.iter().enumerate() {
        print!(" {: <w$}", header, w = col_widths[i]);
        print!("\u{2502}");
    }
    println!();

    print_separator(&col_widths, "\u{251c}", "\u{253c}", "\u{2524}");

    for row in rows {
        print!("\u{2502}");
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                print!(" {: <w$}", cell, w = col_widths[i]);
            }
            print!("\u{2502}");
        }
        println!();
    }

    print_separator(&col_widths, "\u{2514}", "\u{2534}", "\u{2518}");
}

fn print_separator(widths: &[usize], left: &str, mid: &str, right: &str) {
    print!("{left}");
    for (i, w) in widths.iter().enumerate() {
        print!("{}", "\u{2500}".repeat(w + 2));
        if i < widths.len() - 1 {
            print!("{mid}");
        }
    }
    println!("{right}");
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn collect_keys(items: &[Value]) -> Vec<String> {
    let mut key_set = std::collections::BTreeSet::new();
    for item in items {
        if let Value::Object(map) = item {
            for k in map.keys() {
                key_set.insert(k.clone());
            }
        }
    }
    let mut keys: Vec<String> = key_set.into_iter().collect();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn print_json_output() {
        let value = json!({"validators": [{"id": 1, "status": "active"}]});
        print_output(&value, true);
    }

    #[test]
    fn print_tabular_output() {
        let value = json!([
            {"id": 1, "status": "active", "ed25519_key": "abcd"},
            {"id": 2, "status": "inactive", "ed25519_key": "ef01"}
        ]);
        print_output(&value, false);
    }

    #[test]
    fn print_empty_array() {
        let value = json!([]);
        print_output(&value, false);
    }

    #[test]
    fn print_single_object() {
        let value = json!({"result": "ok", "count": 42});
        print_output(&value, false);
    }
}
