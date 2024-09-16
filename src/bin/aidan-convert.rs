use rusty_halloween::{
    prelude::{
        pack::{DrawPack, HeaderPack},
        FrameSendPack, MessageSendPack,
    },
    show::LaserDataFrame,
};
use serde_json::{Map, Value};
use std::fs;

fn main() {
    // Read the JSON file
    let json_content = fs::read_to_string("src/bin/aidan.json").expect("Failed to read the file");

    // Parse the JSON content
    let json: Value = serde_json::from_str(&json_content).expect("Failed to parse JSON");

    // Iterate through each shape in the JSON
    if let Value::Object(shapes) = json {
        let mut new_json = Map::new();

        for (shape_name, shape_data) in shapes {
            println!("Shape: {}", shape_name);

            // Check if the shape_data is an array
            if let Value::Array(points) = shape_data {
                let message_send_pack: MessageSendPack = MessageSendPack::new(
                    HeaderPack::default(),
                    points
                        .iter()
                        .enumerate()
                        .map(|(index, point)| {
                            let point_data = point.as_object().unwrap();

                            let is_white =
                                match point_data.get("hex").and_then(Value::as_str).unwrap() {
                                    "fff" => true,
                                    "000" => false,
                                    _ => unreachable!(),
                                };

                            LaserDataFrame {
                                x_pos: point_data.get("x").and_then(Value::as_u64).unwrap() as u16,
                                y_pos: point_data.get("y").and_then(Value::as_u64).unwrap() as u16,
                                r: if is_white { 0 } else { 255 },
                                g: if is_white { 0 } else { 255 },
                                b: if is_white { 0 } else { 255 },
                            }
                        })
                        .collect(),
                );

                let frame_send_pack: FrameSendPack = message_send_pack.into();

                // println!("{:?}", frame_send_pack.into_bytes());

                let bytes = frame_send_pack.into_bytes();

                // We want to convert the bytes to a hex string
                let hex_string = bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<String>>()
                    .join("");

                // println!("{}", hex_string);

                new_json.insert(shape_name, Value::String(hex_string));
            }

            println!(); // Add a blank line between shapes
        }
 
        // Convert the new JSON object to a string
        let new_json_string = serde_json::to_string_pretty(&Value::Object(new_json))
            .expect("Failed to serialize JSON");

        // Write the new JSON to aidan2.json
        fs::write("src/bin/aidan2.json", new_json_string).expect("Failed to write to file");
    }

    println!("Parsing complete.");
}
