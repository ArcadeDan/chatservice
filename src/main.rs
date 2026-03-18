use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::net::{self, UnixListener, UnixStream},
    sync::{Arc, Mutex},
    thread,
};

fn main() {
    // -- SETUP --
    let socket_path = "/tmp/chatservice.sock";
    let _ = fs::remove_file(socket_path); // remove socket from last session

    // creates socket file and listens for incoming connections
    let listener = UnixListener::bind(socket_path).expect("Failed to bind to socket");
    listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking mode");

    println!("Chat server listening on {}", socket_path);

    let mut clients: Vec<UnixStream> = Vec::new();

    loop {
        // accept new connections
        match listener.accept() {
            Ok((stream, _addr)) => {
                println!("New client connected");
                stream
                    .set_nonblocking(true)
                    .expect("Failed to set non-blocking mode");
                clients.push(stream);
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // No new connections, continue to check existing clients
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }

        // read messages from clients
        let mut messages: Vec<String> = Vec::new();
        let mut disconnected: Vec<usize> = Vec::new();

        for (i, client) in clients.iter().enumerate() {
            let mut reader = BufReader::new(client);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    println!("Client disconnected");
                    disconnected.push(i);
                }
                Ok(_) => {
                    let msg = line.trim().to_string();
                    println!("Received: {}", msg);
                    messages.push(msg);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // No data to read, continue
                }
                Err(e) => {
                    eprintln!("Client disconnected");
                    disconnected.push(i);
                }
            }
        }

        // remove disconnected clients
        for i in disconnected.into_iter().rev() {
            clients.remove(i);
        }

        for msg in &messages {
            clients.retain(|mut client| writeln!(client, "{}", msg).is_ok())
        }

        std::thread::sleep(std::time::Duration::from_millis(10)); // avoid busy waiting
    }
}
