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

impl Client {
    fn try_clone(&self) -> std::io::Result<Client> {
        match self {
            Client::Unix(stream) => Ok(Client::Unix(stream.try_clone()?)),
            Client::Tcp(stream) => Ok(Client::Tcp(stream.try_clone()?)),
        }
    }

    fn print_usage() {
        println!("Usage:");
        println!("  - To connect as a client: socat - UNIX-CONNECT:/tmp/chatservice.sock");
        println!("  - To connect via TCP: telnet {}:{}", HOST, PORT);
    }
}

fn run_host() {
    // -- SETUP --

    let socket_path = "/tmp/chatservice.sock";
    let tcp_addr = format!("{}:{}", HOST, PORT);

    let _ = fs::remove_file(socket_path); // remove socket from last session

    // creates socket file and listens for incoming connections
    let unix_listener = UnixListener::bind(socket_path).expect("Failed to bind to socket");
    unix_listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking mode");

    let tcp_listener = TcpListener::bind(&tcp_addr).expect("Failed to bind to TCP address");
    tcp_listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking mode");

    println!("Chat server listening on {} and {}", socket_path, tcp_addr);

    let mut clients: Vec<Client> = Vec::new();

    loop {
        // accept new connections
        match unix_listener.accept() {
            Ok((stream, _addr)) => {
                println!("New client connected");
                stream
                    .set_nonblocking(true)
                    .expect("Failed to set non-blocking mode");
                clients.push(Client::Unix(stream));
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // No new connections, continue to check existing clients
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }

        match tcp_listener.accept() {
            Ok((stream, addr)) => {
                println!("New TCP client connected from: {}", addr);
                stream
                    .set_nonblocking(true)
                    .expect("Failed to set non-blocking mode");
                clients.push(Client::Tcp(stream));
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // No new connections, continue to check existing clients
            }
            Err(e) => {
                eprintln!("Error accepting TCP connection: {}", e);
            }
        }

        // read messages from clients
        let mut messages: Vec<String> = Vec::new();
        let mut disconnected: Vec<usize> = Vec::new();

        for (i, client) in clients.iter_mut().enumerate() {
            let mut buf = [0u8; 1024];
            match client.read(&mut buf) {
                Ok(0) => {
                    // Client disconnected
                    println!("Client disconnected");
                    disconnected.push(i);
                }
                Ok(_) => {
                    let msg = String::from_utf8_lossy(&buf).trim().to_string();
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
            clients.retain_mut(|client| writeln!(client, "{}", msg).is_ok())
        }

        std::thread::sleep(std::time::Duration::from_millis(10)); // avoid busy waiting
    }
}

fn set_stdin_nonblocking() {
    unsafe {
        let fd = std::io::stdin().as_raw_fd();
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

fn run_client(mut stream: Client) {
    println!("Connected to chat server. Type messages and press Enter.");
    set_stdin_nonblocking();

    match &stream {
        Client::Unix(s) => s
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode"),
        Client::Tcp(s) => s
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode"),
    }

    let stdin = std::io::stdin();
    let mut stdin_reader = BufReader::new(stdin);

    loop {
        let mut buf = [0u8; 1024];
        match stream.read(&mut buf) {
            Ok(0) => {
                println!("Server disconnected");
                break;
            }
            Ok(n) => {
                let msg = String::from_utf8_lossy(&buf[..n]);
                println!("> {}", msg);
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // no data
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
