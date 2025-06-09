use tokio::{
    net::TcpStream, 
    io::{AsyncReadExt, AsyncWriteExt}, 
    time::sleep, time::Duration
};

#[tokio::main]
async fn main() {
    let mut stream = TcpStream::connect("127.0.0.1:7000").await.unwrap();
    let duration: u32 = 7;
    let send_at = duration - 1;
    stream.write_all(&duration.to_be_bytes()).await;
    
    loop {
        stream.write_all(b"HB").await.unwrap();

        let mut response = [0u8; 2];
        match stream.read_exact(&mut response).await {
            Ok(_) if &response == b"OK" => println!("Server is alive"),
            _ => {
                println!("No response. Server might be down.");
                break;
            }
        }

        sleep(Duration::from_secs(send_at.into())).await;
    }

}
