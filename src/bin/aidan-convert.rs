use rusty_halloween::{
    prelude::{pack::HeaderPack, FrameSendPack, MessageSendPack},
    show::LaserDataFrame,
};
use serde_json::{Map, Value};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let json_content = fs::read_to_string("src/bin/aidan.json")?;
    // let json: Value = serde_json::from_str(&json_content)?;

    // let new_json = json
    //     .as_object()
    //     .ok_or("Invalid JSON structure")?
    //     .iter()
    //     .filter_map(|(shape_name, shape_data)| {
    //         shape_data.as_array().map(|points| {
    //             let message_send_pack = MessageSendPack::new(
    //                 HeaderPack::default(),
    //                 points
    //                     .iter()
    //                     .filter_map(|point| {
    //                         point.as_object().map(|point_data| {
    //                             let is_white =
    //                                 point_data.get("hex").and_then(Value::as_str) == Some("fff");
    //                             LaserDataFrame {
    //                                 pattern_id: 0,
    //                                 r: if is_white { 255 } else { 0 },
    //                                 g: if is_white { 255 } else { 0 },
    //                                 b: if is_white { 255 } else { 0 },
    //                             }
    //                         })
    //                     })
    //                     .collect(),
    //             );

    //             let frame_send_pack: FrameSendPack = message_send_pack.into();
    //             let hex_string = frame_send_pack
    //                 .into_bytes()
    //                 .iter()
    //                 .map(|b| format!("{:02x}", b))
    //                 .collect::<String>();

    //             (shape_name.clone(), Value::String(hex_string))
    //         })
    //     })
    //     .collect::<Map<String, Value>>();

    // let new_json_string = serde_json::to_string_pretty(&Value::Object(new_json))?;
    // fs::write("src/bin/aidan2.json", new_json_string)?;

    Ok(())
}
