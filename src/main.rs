use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    os::{
        fd::AsRawFd,
        unix::net::{self, UnixListener, UnixStream},
    },
    sync::{Arc, Mutex},
    thread,
};

const PORT: u16 = 13952;
const HOST: &str = "127.0.0.1";

// We need a unified type to represent both Unix and TCP clients
enum Client {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Read for Client {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Client::Unix(stream) => stream.read(buf),
            Client::Tcp(stream) => stream.read(buf),
        }
    }
}

impl Write for Client {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Client::Unix(stream) => stream.write(buf),
            Client::Tcp(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Client::Unix(stream) => stream.flush(),
            Client::Tcp(stream) => stream.flush(),
        }
    }
}

fn run_host() {
    // -- SETUP --
    let socket_path = "/tmp/chatservice.sock";
    let tcp_addr = format!("{}:{}", HOST, PORT);

    let _ = fs::remove_file(socket_path); // remove socket from last session

    // creates socket file and listens for incoming connections
    let unix_listener = UnixListener::bind(socket_path).expect("Failed to bind to socket"); // /tmp/chatservice.sock
    unix_listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking mode");


    // bind a TCP socket to 127.0.0.1:13952 and listen for incoming connections
    let tcp_listener = TcpListener::bind(&tcp_addr).expect("Failed to bind to TCP address"); // 127.0.0.1:13952
    tcp_listener
        .set_nonblocking(true) // we need non-blocking because we are handling for both UNIX and TCP listeners in the same loop
        .expect("Failed to set non-blocking mode");

    println!("Chat server listening on {} and {}", socket_path, tcp_addr);

    let mut clients: Vec<Client> = Vec::new();

    loop {
        // accept new connections
        match unix_listener.accept() {
            Ok((stream, _addr)) => { // if a client connects to the UNIX socket
                println!("New client connected");
                stream
                    .set_nonblocking(true)
                    .expect("Failed to set non-blocking mode");
                clients.push(Client::Unix(stream)); // add the new client to our list of clients
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // No new connections, continue to check existing clients
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }

        match tcp_listener.accept() {
            Ok((stream, addr)) => { // if a client connects to the TCP socket
                println!("New TCP client connected from: {}", addr);
                stream
                    .set_nonblocking(true)
                    .expect("Failed to set non-blocking mode");
                clients.push(Client::Tcp(stream)); // add the new client to our list of clients
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // No new connections, continue to check existing clients
            }
            Err(e) => {
                eprintln!("Error accepting TCP connection: {}", e);
            }
        }

        // read messages from clients
        let mut messages: Vec<String> = Vec::new(); // collect all messages in an iteration
        let mut disconnected: Vec<usize> = Vec::new(); // for removing disconnected clients after the loop

        for (i, client) in clients.iter_mut().enumerate() { // track index for removing disconnected clients later
            let mut buf = [0u8; 1024]; // 1024 byte byte buffer
            match client.read(&mut buf) {
                Ok(0) => { // 0 bytes read means the client has disconnected and we mark it for removal
                    // Client disconnected
                    println!("Client disconnected");
                    disconnected.push(i);
                }
                Ok(n) => { // client sent data, we convert it to a string and add it to the list of messages to broadcast
                    let msg = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                    println!("Received: {}", msg);
                    messages.push(msg);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // No data to read, continue (client is connected but hasn't sent anything)
                }
                Err(e) => {
                    eprintln!("Client disconnected");
                    disconnected.push(i);
                }
            }
        }

        // remove disconnected clients
        for i in disconnected.into_iter().rev() { // we reverse the indices to remove from the end first to avoid shifting issues
            clients.remove(i);
        }

        for msg in &messages { // iterate over whole collection of messages
            clients.retain_mut(|client| writeln!(client, "{}", msg).is_ok()) // we keep only clients where closure is true
        }

        std::thread::sleep(std::time::Duration::from_millis(10)); // avoid busy waiting
    }
}

// We need to set stdin to non-blocking mode so we can read user input without blocking the main loop
fn set_stdin_nonblocking() {
    unsafe {
        let fd = std::io::stdin().as_raw_fd(); // get the file descriptor for stdin
        let flags = libc::fcntl(fd, libc::F_GETFL); // get the current flags for stdin
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK); // set the O_NONBLOCK flag to make stdin non-blocking
    }
}

fn run_client(mut stream: Client) {
    println!("Connected to chat server. Type messages and press Enter.");
    set_stdin_nonblocking(); // set stdin to non-blocking mode

    match &stream { // set both unix and tcp streams to non-blocking
        Client::Unix(s) => s
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode"),
        Client::Tcp(s) => s
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode"),
    }

    let stdin = std::io::stdin();
    let mut stdin_reader = BufReader::new(stdin); // wrap stdin in a BufReader to read lines of input

    loop {
        let mut buf = [0u8; 1024];
        match stream.read(&mut buf) { // non blocking 1024 byte buffer read from the server
            Ok(0) => { // server closed the connection
                println!("Server disconnected");
                break;
            }
            Ok(n) => { // server sent data
                let msg = String::from_utf8_lossy(&buf[..n]);
                println!("> {}", msg);
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // no data from server
            }
            Err(e) => {
                eprintln!("Connection error: {}", e);
                break;
            }
        }

        let mut line = String::new();
        match stdin_reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if write!(stream, "{}", line).is_err() {
                    eprintln!("Failed to send message");
                    break;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // no input
            }
            Err(e) => break,
        }

        std::thread::sleep(std::time::Duration::from_millis(10)); // avoid busy waiting
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [_, "host"] => run_host(),
        [_, "client"] => {
            let addr = format!("{}:{}", HOST, PORT);
            let stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
                eprintln!("Failed to connect to {}: {}", addr, e);
                std::process::exit(1);
            });
            println!("Connected to TCP server at {}", addr);
            run_client(Client::Tcp(stream));
        }
        _ => {
            eprintln!("Usage:");
            eprintln!("  {} host -- start the chat server", args[0]);
            eprintln!("  {} client -- connect to the chat server", args[0]);
            std::process::exit(1);
        }
    }
}
