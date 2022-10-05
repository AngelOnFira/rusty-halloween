use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use std::io::{self, prelude::*, BufReader, Error};

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
    let listener = LocalSocketListener::bind("/tmp/example.sock")?;

    for mut conn in listener.incoming().filter_map(handle_error) {
        conn.write_all(b"Hello from server!\n")?;
        let mut conn = BufReader::new(conn);
        let mut buffer = String::new();
        conn.read_line(&mut buffer)?;
        println!("Client answered: {}", buffer);
    }

    Ok(())
}
