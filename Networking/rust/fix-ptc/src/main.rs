use tokio::{
    net::TcpListener, 
    io::{AsyncReadExt, AsyncWriteExt}, 
    time::{timeout, Duration}
};

use std::net::Shutdown;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:7000").await.unwrap();
    
    
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut dur = [0u8; 4];
    let timer;
    let res1 = timeout(Duration::from_secs(5), socket.read_exact(&mut dur)).await;

    match res1 {
        Ok(Ok(_)) =>{
            timer = u32::from_be_bytes(dur);
            println!("Duration recieved! ({})", timer);
        }
        _ => {
            println!("Invalid connection: no heartbeat set");
            return;
        }
            //  stream.shutdown(std::net::Shutdown::Write).expect("Shutdown failed");
    }
     
    
    

    tokio::spawn(async move {
        let mut count = 0;
        while count < 3 {
            let mut buf = [0u8; 2];
            let res2 = timeout(Duration::from_secs(timer.into()), socket.read_exact(&mut buf)).await;

            match res2 {
                Ok(Ok(_)) if &buf == b"HB" => {
                    println!("Heartbeat received.");
                    socket.write_all(b"OK").await.unwrap();
                    
                },
                _ => {
                    println!("Connection lost or timeout.");
                    return;
                }
            }
            count += 1;
        }
        socket.shutdown().await.expect("Shutdown failed");
        socket.flush().await.expect("Flush failed");
        drop(socket);
    }).await;
}

