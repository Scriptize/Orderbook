use tokio::{
    net::TcpListener, 
    io::{AsyncReadExt, AsyncWriteExt}, 
    time::{timeout, Duration}
};

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:7000").await.unwrap();

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            loop {
                let mut buf = [0u8; 2];
                let res = timeout(Duration::from_secs(10), socket.read_exact(&mut buf)).await;

                match res {
                    Ok(Ok(_)) if &buf == b"HB" => {
                        println!("Heartbeat received.");
                        socket.write_all(b"OK").await.unwrap();
                        
                    },
                    _ => {
                        println!("Connection lost or timeout.");
                        return;
                    }
                }
            }
        });
    }
}
