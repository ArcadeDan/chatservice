// Dan Sparks

use std::{
    collections::HashMap,
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
const PASSWORDS_PATH: &str = "passwords.txt";

const MAX_MESSAGE_SIZE: usize = 256;
const MIN_MESSAGE_SIZE: usize = 1;

const MAX_USERNAME_LEN: usize = 32;
const MIN_USERNAME_LEN: usize = 3;

const MAX_PASSWORD_LEN: usize = 8;
const MIN_PASSWORD_LEN: usize = 4;

const MAX_CLIENTS: usize = 3;

// START OF LOGIN FUNCTIONALITY
struct AuthenticatedClient {
    username: String,
    stream: Client,
}

// we load users and passwords from the path and collect it into a hashmap
fn load_users() -> HashMap<String, String> {
    let contents = fs::read_to_string(PASSWORDS_PATH).unwrap_or_else(|e| {
        eprintln!("Failed to read passwords file: {}", e);
        std::process::exit(1);
    });

    contents
        .lines()
        .filter(|line| !line.trim().is_empty()) // skip blank lines
        .filter_map(|line| {
            let (user, pass) = line.split_once(':')?; // split on the first colon
            Some((user.trim().to_string(), pass.trim().to_string())) // trim whitespace and
        })
        .collect()
}

fn save_user(username: &str, password: &str) -> std::io::Result<()> {
    if let Ok(contents) = fs::read_to_string(PASSWORDS_PATH) {
        if !contents.is_empty() && !contents.ends_with("\n") {
            let mut file = fs::OpenOptions::new().append(true).open(PASSWORDS_PATH)?;
            writeln!(file)?;
        }
    }

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(PASSWORDS_PATH)?;
    writeln!(file, "{}:{}", username.trim(), password.trim())?;
    Ok(())
}

fn validate_username(username: &str) -> Result<(), String> {
    if username.len() < MIN_USERNAME_LEN {
        return Err(format!(
            "Username must be at least {} characters long",
            MIN_USERNAME_LEN
        ));
    }
    if username.len() > MAX_USERNAME_LEN {
        return Err(format!(
            "Username must be at most {} characters long",
            MAX_USERNAME_LEN
        ));
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(format!(
            "Password must be at least {} characters long",
            MIN_PASSWORD_LEN
        ));
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(format!(
            "Password must be at most {} characters long",
            MAX_PASSWORD_LEN
        ));
    }
    Ok(())
}

fn handle_login(stream: &mut Client) -> Option<String> {
    match stream {
        Client::Unix(s) => s.set_nonblocking(false).ok(),
        Client::Tcp(s) => s.set_nonblocking(false).ok(),
    };
    loop {
        let users = load_users();
        let mut buf = [0u8; 1024];
        let n = match stream.read(&mut buf) {
            Ok(0) | Err(_) => return None,
            Ok(n) => n,
        };

        let input = String::from_utf8_lossy(&buf[..n]).trim().to_string();
        let mut parts: Vec<&str> = input.splitn(3, ' ').collect();

        match parts.as_slice() {
            ["login", user_id, password] => {
                if let Err(e) = validate_username(user_id) {
                    let _ = stream.write(format!("ERR {}\n", e).as_bytes());
                    continue;
                }
                if let Err(e) = validate_password(password) {
                    let _ = stream.write(format!("ERR {}\n", e).as_bytes());
                    continue;
                }
                if users.get(*user_id).map_or(false, |p| p == password) {
                    let _ = stream.write(format!("OK {}\n", user_id).as_bytes());
                    println!("{} login", user_id);
                    return Some(user_id.to_string());
                } else {
                    let _ = stream.write(b"Denied. User name or password incorrect.\n");
                    println!("Failed login attempt for {}", user_id);
                    continue;
                }
            }
            ["newuser", user_id, password] => {
                if let Err(e) = validate_username(user_id) {
                    let _ = stream.write(format!("ERR {}\n", e).as_bytes());
                    continue;
                }
                if let Err(e) = validate_password(password) {
                    let _ = stream.write(format!("ERR {}\n", e).as_bytes());
                    continue;
                }
                if users.contains_key(*user_id) {
                    let _ = stream.write(b"Denied. User account already exists\n");
                    continue;
                }
                if save_user(user_id, password).is_err() {
                    let _ = stream.write(b"ERR Failed to create user\n");
                    continue;
                }
                let _ =
                    stream.write(format!("New user account created. Please login.\n").as_bytes());
                println!("New user account created");
                continue;
            }
            _ => {
                let _ = stream.write(b"Denied. Please login first.\n");
                continue;
            }
        }
    }
}
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

    // verify if password file exists on startup
    let users = load_users();
    println!("Loaded {} users from {}", users.len(), PASSWORDS_PATH);

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

    let clients: Arc<Mutex<Vec<AuthenticatedClient>>> = Arc::new(Mutex::new(Vec::new())); // shared state of clients across threads with lock

    println!("My chat room server. Version Two.\n");

    let clients_clone = Arc::clone(&clients);
    thread::spawn(move || {
        loop {
            let mut messages: Vec<String> = Vec::new();
            let mut disconnected: Vec<usize> = Vec::new();
            let mut who_requests: Vec<usize> = Vec::new(); // track who request "who" to get past borrow checker

            {
                let mut clients = clients_clone.lock().unwrap();
                for (i, client) in clients.iter_mut().enumerate() {
                    let mut buf = [0u8; 1024];
                    match client.stream.read(&mut buf) {
                        Ok(0) => {
                            let msg = format!("{} left.", client.username);
                            println!("{} logout.", client.username);
                            messages.push(msg);
                            disconnected.push(i);
                        }
                        Ok(n) => {
                            let text = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                            if text.is_empty() {
                                continue;
                            }
                            if text == "logout" {
                                let msg = format!("{} left.", client.username);
                                println!("{} logout.", client.username);
                                messages.push(msg);
                                disconnected.push(i);
                            } else if text == "who" {
                                who_requests.push(i);
                            } else {
                                let msg = format!("{}: {}", client.username, text);
                                println!("{}", msg);
                                messages.push(msg);
                            }
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => {} // no message from this client
                        Err(_) => {
                            let msg = format!("{} left.", client.username);
                            println!("{}", client.username);
                            messages.push(msg);
                            disconnected.push(i);
                        }
                    }
                }

                // THIS WHOLE BLOCK EXISTS BECAUSE OF THE BORROW CHECKER.
                if !who_requests.is_empty() {
                    let names: Vec<&str> = clients.iter().map(|c| c.username.as_str()).collect();
                    let response = format!("{}\n", names.join(", "));
                    for &i in &who_requests {
                        let _ = clients[i].stream.write(response.as_bytes());
                        let _ = clients[i].stream.flush();
                    }
                }

                // disconnected client removal
                for i in disconnected.into_iter().rev() {
                    clients.remove(i);
                }

                //broadcast message
                for msg in &messages {
                    clients.retain_mut(|client| write!(client.stream, "{}\n", msg).is_ok());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10)); // avoid busy waiting
        }
    });

    // for main thread to accept new connections and spawwnn more threads

    loop {
        // accept new unix clients
        match unix_listener.accept() {
            Ok((stream, _addr)) => {
                let clients = Arc::clone(&clients);
                thread::spawn(move || {
                    let mut client = Client::Unix(stream);
                    if let Some(username) = handle_login(&mut client) {
                        if let Client::Unix(s) = &client {
                            s.set_nonblocking(true)
                                .expect("Failed to set non-blocking mode");
                        }
                        let mut clients = clients.lock().unwrap();
                        clients.push(AuthenticatedClient {
                            username,
                            stream: client,
                        });
                    }
                });
            }

            Err(e) if e.kind() == ErrorKind::WouldBlock => {} // no new unix client
            Err(e) => eprintln!("Error accepting unix connection: {}", e),
        }

        // accept new tcp clients
        match tcp_listener.accept() {
            Ok((stream, _addr)) => {
                let clients = Arc::clone(&clients);
                thread::spawn(move || {
                    let mut client = Client::Tcp(stream);
                    if let Some(username) = handle_login(&mut client) {
                        if let Client::Tcp(s) = &client {
                            s.set_nonblocking(true)
                                .expect("Failed to set non-blocking mode");
                        }
                        let mut clients = clients.lock().unwrap();
                        clients.push(AuthenticatedClient {
                            username,
                            stream: client,
                        });
                    }
                });
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {} // no new tcp client
            Err(e) => eprintln!("Error accepting TCP connection: {}", e),
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

fn client_send_auth(stream: &mut Client, input: &str) -> bool {
    let parts: Vec<&str> = input.trim().splitn(3, ' ').collect();

    let (command, username, password) = match parts.as_slice() {
        ["login", user_id, password] => ("login", *user_id, *password),
        ["newuser", user_id, password] => ("newuser", *user_id, *password),
        ["login", ..] | ["newuser", ..] => {
            eprintln!("Usage: login <username> <password> or newuser <username> <password>");
            return false;
        }
        _ => {
            eprintln!("Denied. Please login first.");
            return false;
        }
    };

    // client side validation of username and password before sending to the server
    if let Err(e) = validate_username(username) {
        eprintln!("{}", e);
        return false;
    }
    if let Err(e) = validate_password(password) {
        eprintln!("{}", e);
        return false;
    }

    //send command to server
    let cmd = format!("{} {} {}\n", command, username, password);
    if stream.write(cmd.as_bytes()).is_err() {
        eprintln!("Failed to send command to server");
        return false;
    }

    let mut buf = [0u8; 1024];
    match stream.read(&mut buf) {
        Ok(0) => {
            eprintln!("Server closed connection");
            false
        }
        Ok(n) => {
            let response = String::from_utf8_lossy(&buf[..n]).trim().to_string();
            if response.starts_with("OK") {
                println!("{}", response);
                command == "login"
            } else {
                eprintln!("{}", response);
                false
            }
        }
        Err(e) => {
            eprintln!("Error reading from server: {}", e);
            false
        }
    }
}

fn run_client(mut stream: Client) {
    println!("My chat room client. Version One.\n");

    let stdin = std::io::stdin();
    let mut stdin_reader = BufReader::new(stdin.lock());
    let mut logged_in_user = String::new();

    loop {
        let _ = std::io::stdout().flush(); // flush stdout to ensure prompt is shown before input
        let mut line = String::new();
        match stdin_reader.read_line(&mut line) {
            Ok(0) => return, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                if let [_, username, _] = parts.as_slice() {
                    let name = username.to_string();
                    if client_send_auth(&mut stream, trimmed) {
                        logged_in_user = name;
                        break;
                    }
                } else {
                    let _ = client_send_auth(&mut stream, trimmed);
                }
                // if login failed, prompt again
            }

            Err(_) => return,
        }
    }

    drop(stdin_reader); // drop the stdin reader to release the lock on stdin
    println!("login confirmed");

    set_stdin_nonblocking(); // set stdin to non-blocking mode

    match &stream {
        // set both unix and tcp streams to non-blocking
        Client::Unix(s) => s
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode"),
        Client::Tcp(s) => s
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode"),
    }

    let mut input_buf = String::new(); // accumulate stdin input until we get a full line

    loop {
        let mut buf = [0u8; 1024];
        match stream.read(&mut buf) {
            // non blocking 1024 byte buffer read from the server
            Ok(0) => {
                // server closed the connection
                println!("Server disconnected");
                break;
            }
            Ok(n) => {
                // server sent data
                let msg = String::from_utf8_lossy(&buf[..n]);
                print!("{}", msg);
                let _ = std::io::stdout().flush();
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                // no data from server
            }
            Err(e) => {
                eprintln!("Connection error: {}", e);
                break;
            }
        }

        let mut raw = [0u8; 256]; // raw buffer for stdin bytes
        let stdin = std::io::stdin(); // handle
        let mut handle = stdin.lock(); // lock stdin
        match handle.read(&mut raw) {
            Ok(0) => break, // EOF
            Ok(n) => {
                // got n bytes from stdin, we append it to the input buffer
                input_buf.push_str(&String::from_utf8_lossy(&raw[..n]));
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {} // no input from user
            Err(_) => break,
        }
        drop(handle);

        while let Some(pos) = input_buf.find('\n') {
            let line: String = input_buf.drain(..=pos).collect();
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with("login") || trimmed.starts_with("newuser") {
                eprintln!("You are already logged in.");
                continue;
            }

            if trimmed == "logout" {
                let _ = stream.write(b"logout\n");
                let _ = stream.flush();
                println!("{} left.", logged_in_user);
                return;
            }

            if trimmed == "who" {
                if stream.write(b"who").is_err() || stream.flush().is_err() {
                    eprintln!("Failed to send command to server");
                    return;
                }
                continue;
            }

            if !trimmed.starts_with("send all") {
                eprintln!("Usage: send all <message>");
                continue;
            }

            let message = &trimmed["send all ".len()..];

            if message.is_empty() {
                eprintln!("Message cannot be empty");
                continue;
            }

            if message.len() > MAX_MESSAGE_SIZE {
                eprintln!(
                    "Message must be at most {} characters long",
                    MAX_MESSAGE_SIZE
                );
                continue;
            }

            if stream.write(message.as_bytes()).is_err() || stream.flush().is_err() {
                eprintln!("Failed to send message");
                return;
            }
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
