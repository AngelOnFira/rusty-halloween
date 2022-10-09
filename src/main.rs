use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use proto_schema::schema::PicoMessage;
use protobuf::Message;
use std::io::{self, prelude::*, BufReader, Error};

mod audio;
mod lights;
mod pico;
mod projector;
mod proto_schema;

fn handle_error(conn: io::Result<LocalSocketStream>) -> Option<LocalSocketStream> {
    match conn {
        Ok(val) => Some(val),
        Err(error) => {
            eprintln!("Incoming connection failed: {}", error);
            None
        }
    }
}

fn main() -> Result<(), Error> {
    // Make sure the socket is removed if the program exits
    std::fs::remove_file("/tmp/pico.sock").ok();

    let listener = LocalSocketListener::bind("/tmp/pico.sock")?;

    for mut conn in listener.incoming().filter_map(handle_error) {
        // Recieve the data
        // let mut conn = BufReader::new(conn);
        // let mut buffer = String::new();
        // conn.read_line(&mut buffer)?;

        // Try to decode it as protobuf
        // TODO: Reply with an error if this fails
        let proto = PicoMessage::parse_from_reader(&mut conn).unwrap();

        // Print the message
        println!("{:#?}", proto);

        // Translate it to the projector protocol
    }

    Ok(())
}
