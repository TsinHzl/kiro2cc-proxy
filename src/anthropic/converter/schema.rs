// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! JSON Schema 规范化：$ref 展开、Kiro 严格模式清洗

/// 规范化 JSON Schema，修复 MCP/Agent SDK 工具定义中常见的类型问题。
///
/// Kiro 上游对工具 schema 比 Anthropic 更严格，`required: null`、`properties: null`、
/// 嵌套属性不是 object、`items` 不是 schema，以及复杂 JSON Schema 关键字都可能触发
/// 400 "Improperly formed request"。
pub(super) fn normalize_json_schema(schema: serde_json::Value) -> serde_json::Value {
    // 先就地展开 $ref（依赖 $defs/definitions），再规范化。
    // Kiro 不认 $ref，未展开会让 MCP/pydantic/zod 工具的参数约束静默丢失，
    // 该属性退化为无约束空对象。
    let defs = extract_schema_defs(&schema);
    // 总是运行 resolve：即便没有 $defs，也需把无法展开的 $ref（OpenAPI/外部形式）
    // 显式降级为宽松 object，否则它们会被后续 retain 白名单清成空壳。
    let resolved = resolve_schema_refs(schema, &defs, 0);
    normalize_json_schema_inner(resolved, true)
}

/// 提取顶层 `$defs` / `definitions` 作为 $ref 解析表。
fn extract_schema_defs(schema: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut defs = serde_json::Map::new();
    if let Some(obj) = schema.as_object() {
        for key in ["$defs", "definitions"] {
            if let Some(serde_json::Value::Object(m)) = obj.get(key) {
                for (k, v) in m {
                    defs.insert(k.clone(), v.clone());
                }
            }
        }
    }
    defs
}

/// 深度优先展开所有 `$ref`（仅支持 `#/$defs/<name>` 与 `#/definitions/<name>` 两种最常见形式）。
/// `depth` 仅在 $ref 跳转时递增，超过上限视为循环引用，降级为宽松 object 兜底。
fn resolve_schema_refs(
    value: serde_json::Value,
    defs: &serde_json::Map<String, serde_json::Value>,
    depth: usize,
) -> serde_json::Value {
    const MAX_REF_DEPTH: usize = 16;
    if depth > MAX_REF_DEPTH {
        return serde_json::json!({ "type": "object", "additionalProperties": true });
    }
    match value {
        serde_json::Value::Object(mut obj) => {
            if let Some(serde_json::Value::String(ref_str)) = obj.get("$ref") {
                let ref_str = ref_str.clone();
                let name = ref_str
                    .strip_prefix("#/$defs/")
                    .or_else(|| ref_str.strip_prefix("#/definitions/"))
                    .map(str::to_string);
                obj.remove("$ref");
                match name.as_ref().and_then(|n| defs.get(n)) {
                    Some(target) => {
                        // 展开目标后并入同级字段（不覆盖 $ref 旁已有的 description 等）。
                        let resolved = resolve_schema_refs(target.clone(), defs, depth + 1);
                        if let serde_json::Value::Object(robj) = resolved {
                            for (k, v) in robj {
                                obj.entry(k).or_insert(v);
                            }
                        }
                    }
                    None => {
                        // 未命中：OpenAPI 风格（#/components/...）、外部 URL、或指向
                        // 不存在的 def。无法展开，约束只能丢弃；显式标记为宽松 object
                        // 而非留下空壳，并记日志便于排查工具参数约束丢失。
                        tracing::debug!(
                            "$ref 无法展开（非 #/$defs 形式或目标缺失），降级为宽松 object: {}",
                            ref_str
                        );
                        obj.entry("type".to_string())
                            .or_insert(serde_json::Value::String("object".to_string()));
                    }
                }
            }
            let mut new_obj = serde_json::Map::new();
            for (k, v) in obj {
                new_obj.insert(k, resolve_schema_refs(v, defs, depth));
            }
            serde_json::Value::Object(new_obj)
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.into_iter()
                .map(|v| resolve_schema_refs(v, defs, depth))
                .collect(),
        ),
        other => other,
    }
}

fn normalize_json_schema_inner(schema: serde_json::Value, root: bool) -> serde_json::Value {
    let serde_json::Value::Object(mut obj) = schema else {
        return serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": true
        });
    };

    // 去掉 null 字段；Kiro 侧对 null 容忍度很低。
    obj.retain(|_, v| !v.is_null());

    // type（必须是字符串；数组类型取第一个非 null 的基础类型）
    let normalized_type = match obj.remove("type") {
        Some(serde_json::Value::String(s)) => normalize_schema_type(&s),
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .filter_map(|v| v.as_str().and_then(normalize_schema_type))
            .next(),
        _ => None,
    };
    let is_object_schema = root
        || normalized_type.as_deref() == Some("object")
        || (normalized_type.is_none() && obj.contains_key("properties"));

    if is_object_schema {
        obj.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );
    } else if let Some(t) = normalized_type {
        obj.insert("type".to_string(), serde_json::Value::String(t));
    }

    if is_object_schema {
        // properties（object schema 下必须是 object）
        match obj.remove("properties") {
            Some(serde_json::Value::Object(props)) => {
                let mut normalized = serde_json::Map::new();
                for (name, prop_schema) in props {
                    normalized.insert(name, normalize_json_schema_inner(prop_schema, false));
                }
                obj.insert(
                    "properties".to_string(),
                    serde_json::Value::Object(normalized),
                );
            }
            _ => {
                obj.insert(
                    "properties".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
        }

        // required（object schema 下必须是 string 数组）
        let required = match obj.remove("required") {
            Some(serde_json::Value::Array(arr)) => serde_json::Value::Array(
                arr.into_iter()
                    .filter_map(|v| v.as_str().map(|s| serde_json::Value::String(s.to_string())))
                    .collect(),
            ),
            _ => serde_json::Value::Array(Vec::new()),
        };
        obj.insert("required".to_string(), required);
    } else {
        obj.remove("properties");
        obj.remove("required");
    }

    // items（如果存在，必须是 schema；数组形式取第一个 schema）
    if let Some(items) = obj.remove("items") {
        let normalized_items = match items {
            serde_json::Value::Array(arr) => arr
                .into_iter()
                .find(|v| v.is_object())
                .map(|v| normalize_json_schema_inner(v, false)),
            serde_json::Value::Object(_) => Some(normalize_json_schema_inner(items, false)),
            _ => None,
        };
        if let Some(items) = normalized_items {
            obj.insert("items".to_string(), items);
        }
    }

    // Kiro 对组合 schema 的兼容性较差。前面已经处理了常见的 type: ["x", "null"]，
    // 其余 anyOf/oneOf/allOf 直接丢弃，避免上游把整个工具列表判为 malformed。
    obj.remove("anyOf");
    obj.remove("oneOf");
    obj.remove("allOf");

    // additionalProperties（允许 bool 或 object，其他按 true 处理）
    match obj.remove("additionalProperties") {
        Some(serde_json::Value::Object(schema)) => {
            obj.insert(
                "additionalProperties".to_string(),
                normalize_json_schema_inner(serde_json::Value::Object(schema), false),
            );
        }
        Some(serde_json::Value::Bool(value)) => {
            obj.insert(
                "additionalProperties".to_string(),
                serde_json::Value::Bool(value),
            );
        }
        Some(_) => {
            obj.insert(
                "additionalProperties".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        None => {}
    }

    if let Some(description) = obj.remove("description")
        && let Some(description) = description.as_str()
    {
        let description = match description.char_indices().nth(2000) {
            Some((idx, _)) => description[..idx].to_string(),
            None => description.to_string(),
        };
        obj.insert(
            "description".to_string(),
            serde_json::Value::String(description),
        );
    }

    if let Some(enum_value) = obj.remove("enum")
        && let serde_json::Value::Array(values) = enum_value
    {
        let values: Vec<_> = values
            .into_iter()
            .filter(|v| v.is_string() || v.is_number() || v.is_boolean())
            .collect();
        if !values.is_empty() {
            obj.insert("enum".to_string(), serde_json::Value::Array(values));
        }
    }

    obj.retain(|key, _| {
        matches!(
            key.as_str(),
            "type"
                | "properties"
                | "required"
                | "items"
                | "additionalProperties"
                | "description"
                | "enum"
        )
    });

    serde_json::Value::Object(obj)
}

fn normalize_schema_type(raw: &str) -> Option<String> {
    match raw.trim() {
        "object" | "array" | "string" | "number" | "integer" | "boolean" => {
            Some(raw.trim().to_string())
        }
        _ => None,
    }
}
