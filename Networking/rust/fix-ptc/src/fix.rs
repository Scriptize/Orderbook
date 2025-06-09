use tokio::{
    net::TcpStream, 
    io::{AsyncReadExt, AsyncWriteExt}, 
    time::sleep, time::Duration
};

pub struct fix_client {
    target : String,
    is_connected : bool
}

pub struct fix_server {
    addr: String,
}

pub struct fix_message

impl fix_client{

    pub fn new(addr: &str) -> Self {
        Self{
            target : addr,
            is_connected: false,
        }
    }

    pub fn connect() -> Result<(), Box<dyn std::error::Error>> {
        // stream = TcpStream::connect(&addr)?;
        todo!();
    }

    pub fn disconnect() -> Result<(), Box<dyn std::error::Error>> {
        todo!();
    }
}

impl fix_server{

    pub fn new(addr: &str) -> Self {
        Self{
            addr : String,
        }
    }

    pub fn listen() -> Result<(), Box<dyn std::error::Error>> {
        todo!();
    }

    pub fn disconnect() -> Result<(), Box<dyn std::error::Error>> {
        todo!();
    }
}