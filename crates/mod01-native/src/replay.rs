use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

const MAX_FRAMES: u64 = 36_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const BUTTONS: [&str; 12] = [
    "up", "down", "left", "right", "a", "b", "x", "y", "l", "r", "start", "menu",
];

pub fn encode(frames: &[(u32, Value)]) -> Result<Vec<u8>, String> {
    if frames.len() > MAX_FRAMES as usize {
        return Err("replay exceeds 36,000 frames".to_owned());
    }
    let mut compact: Vec<Value> = Vec::new();
    for (frame, input) in frames {
        let item = trace_frame(u64::from(*frame), input)?;
        if let Some(last) = compact.last_mut() {
            let end = last["frame"].as_u64().unwrap_or(0) + last["duration"].as_u64().unwrap_or(1);
            if end == u64::from(*frame)
                && last["controllers"] == item["controllers"]
                && last["pointer"] == item["pointer"]
            {
                last["duration"] = json!(end - last["frame"].as_u64().unwrap_or(0) + 1);
                continue;
            }
        }
        compact.push(item);
    }
    let mut bytes = serde_json::to_vec(&json!({"revision":1,"frames":compact}))
        .map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_BYTES {
        return Err("replay exceeds 8 MiB".to_owned());
    }
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<BTreeMap<u32, Value>, String> {
    if bytes.len() < 27 || bytes.len() > MAX_BYTES {
        return Err("replay size is invalid".to_owned());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if value["revision"] != 1 || value.as_object().is_none_or(|object| object.len() != 2) {
        return Err("replay revision or fields are invalid".to_owned());
    }
    let frames = value["frames"]
        .as_array()
        .ok_or("replay has no frame list")?;
    let mut output = BTreeMap::new();
    let mut previous_end = 0;
    for frame in frames {
        let object = frame.as_object().ok_or("invalid replay frame")?;
        if object
            .keys()
            .any(|key| !["frame", "duration", "controllers", "pointer"].contains(&key.as_str()))
            || !object.contains_key("controllers")
        {
            return Err("replay frame has unknown fields".to_owned());
        }
        let start = frame["frame"].as_u64().ok_or("invalid frame number")?;
        let duration = if object.contains_key("duration") {
            frame["duration"]
                .as_u64()
                .ok_or("invalid replay duration")?
        } else {
            1
        };
        if start < previous_end
            || start >= MAX_FRAMES
            || duration == 0
            || duration > MAX_FRAMES - start
        {
            return Err("replay frame range is invalid".to_owned());
        }
        let controllers = frame["controllers"]
            .as_array()
            .ok_or("replay controllers are invalid")?;
        if controllers.len() > 4 {
            return Err("replay has too many controllers".to_owned());
        }
        let mut input = empty_input();
        let mut ports = BTreeSet::new();
        for controller in controllers {
            if controller.as_object().is_none_or(|object| {
                object.len() != 2 || !object.contains_key("port") || !object.contains_key("buttons")
            }) {
                return Err("replay controller fields are invalid".to_owned());
            }
            let port = controller["port"].as_u64().ok_or("invalid replay port")?;
            if !(1..=4).contains(&port) || !ports.insert(port) {
                return Err("duplicate or invalid replay port".to_owned());
            }
            let buttons = controller["buttons"]
                .as_array()
                .ok_or("invalid replay button list")?;
            let mut seen = BTreeSet::new();
            for button in buttons {
                let name = button.as_str().ok_or("invalid replay button")?;
                if !BUTTONS.contains(&name) || !seen.insert(name) {
                    return Err("duplicate or invalid replay button".to_owned());
                }
                input["controllers"][(port - 1) as usize]["buttons"][name] = json!(true);
            }
        }
        if !frame["pointer"].is_null() {
            let pointer = &frame["pointer"];
            if pointer.as_object().is_none_or(|object| object.len() != 5)
                || pointer["x"].as_u64().is_none_or(|x| x >= 240)
                || pointer["y"].as_u64().is_none_or(|y| y >= 144)
                || ["primary", "secondary", "inside"]
                    .iter()
                    .any(|key| !pointer[key].is_boolean())
            {
                return Err("invalid replay pointer".to_owned());
            }
            input["pointer"] = pointer.clone();
        }
        for offset in 0..duration {
            output.insert((start + offset) as u32, input.clone());
        }
        previous_end = start + duration;
    }
    Ok(output)
}

fn trace_frame(frame: u64, input: &Value) -> Result<Value, String> {
    if frame >= MAX_FRAMES {
        return Err("replay frame number is invalid".to_owned());
    }
    let controllers = input["controllers"]
        .as_array()
        .ok_or("input controllers are invalid")?;
    if controllers.len() != 4 {
        return Err("input requires four controller ports".to_owned());
    }
    let mut packed = Vec::new();
    for (index, controller) in controllers.iter().enumerate() {
        let buttons = BUTTONS
            .iter()
            .filter(|name| controller["buttons"][**name] == true)
            .copied()
            .collect::<Vec<_>>();
        if !buttons.is_empty() {
            packed.push(json!({"port":index+1,"buttons":buttons}));
        }
    }
    Ok(json!({"frame":frame,"controllers":packed,"pointer":input["pointer"]}))
}

pub fn empty_input() -> Value {
    let buttons = BUTTONS
        .iter()
        .map(|name| ((*name).to_owned(), Value::Bool(false)))
        .collect::<serde_json::Map<_, _>>();
    json!({"controllers":[{"buttons":buttons},{"buttons":buttons},{"buttons":buttons},{"buttons":buttons}],
        "pointer":{"x":0,"y":0,"primary":false,"secondary":false,"inside":false}})
}

#[cfg(test)]
mod tests {
    use super::{decode, empty_input, encode};

    #[test]
    fn records_and_replays_repeated_input() {
        let mut first = empty_input();
        first["controllers"][0]["buttons"]["a"] = true.into();
        let bytes = encode(&[(0, first.clone()), (1, first.clone()), (2, empty_input())]).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["frames"][0]["duration"], 2);
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded[&1], first);
        assert_eq!(decoded[&2], empty_input());
    }

    #[test]
    fn rejects_invalid_duration_and_pointer() {
        let invalid = br#"{"revision":1,"frames":[{"frame":0,"duration":"2","controllers":[]}]}"#;
        assert!(decode(invalid).is_err());
        let invalid = br#"{"revision":1,"frames":[{"frame":0,"controllers":[],"pointer":{"x":240,"y":0,"primary":false,"secondary":false,"inside":true}}]}"#;
        assert!(decode(invalid).is_err());
    }
}
