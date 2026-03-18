use std::{
    fs, io::{BufRead, BufReader, Write}, os::unix::net::{self, UnixListener, UnixStream}, sync::{Arc, Mutex}, thread
};

fn main() {
    let socket_path = "/tmp/chatservice.sock";
    let _ = fs::remove_file(socket_path); // remove socket from last session

    // creates socket file and listens for incoming connections
    let listener = UnixListener::bind(socket_path).expect("Failed to bind to socket");
    listener.set_nonblocking(true).expect("Failed to set non-blocking mode");
    
    println!("Chat server listening on {}", socket_path);

    let mut clients: Vec<UnixStream> = Vec::new();

    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                println!("New client connected");
            
            }
        }
    }




    
             
}
