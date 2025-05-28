use std::io;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
// use std::cell::RefCell;

fn main(){
    // S = Sender
    // R = Reciever

    loop {
        println!("Enter S for sender or R for reciever");

        let mut role = String::new();
        io::stdin().read_line(&mut role);
        role = role.trim().to_string();
        

        if let Some(ch) = role.pop(){
           if ch == "S".chars().next().unwrap(){
                println!("Running Sender...");
                run_sender();
                return;
            } else{
                println!("char was {}", ch);
            } 
           if ch == "R".chars().next().unwrap(){
                println!("Running Receiver...");
                run_reciever();
                return;
            }
            continue;
        }
    }
}

fn run_sender() -> Result<(), Box<dyn std::error::Error>>{
    let mut stream = TcpStream::connect("127.0.0.1:7878")?;
    print!("Connected!");
    loop{
        println!("Enter a message to the stream: ");
        let mut message = String::new();
        let mut val = message.clone();
        val.pop();
        if let Some(c) = val.pop(){
            if c == "q".chars().next().unwrap(){
                break;
            }
        }
        io::stdin().read_line(&mut message);
        
        stream.write_all(message.as_bytes())?;
    }
    Ok(())
}

fn run_reciever()-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("Started Listening...");

    for stream in listener.incoming(){
        handle_client(stream?);
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream) {
    let mut buf = [0u8; 1024];
    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 {
            break;
        }
        println!("Received: {}", String::from_utf8_lossy(&buf[..n]));
    }
}
