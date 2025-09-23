use orderbook::orderbook::{Orderbook, Order, OrderType, Side};
use serde::{Deserialize, Serialize};
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
// use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Debug)]
enum ServerMsg {
    Ack { id: u32 },
    Err(String),
}

fn read_frame(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

fn write_frame(stream: &mut TcpStream, buf: &[u8]) -> std::io::Result<()> {
    let len = (buf.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(buf)?;
    stream.flush()?;
    Ok(())
}

fn main() -> std::io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:9000")?;
    eprintln!("CLIENT: connected");

    let orders = [
        Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 10100, 5),
        Order::new(OrderType::GoodTillCancel, 2, Side::Sell, 10100, 5),
        Order::new(OrderType::GoodTillCancel, 3, Side::Buy, 10200, 5),
        Order::new(OrderType::GoodTillCancel, 4, Side::Sell, 10200, 5),
        Order::new(OrderType::GoodTillCancel, 5, Side::Buy, 10300, 5),
        Order::new(OrderType::GoodTillCancel, 6, Side::Sell, 10300, 5),
        Order::new(OrderType::GoodTillCancel, 7, Side::Buy, 10400, 5),
        Order::new(OrderType::GoodTillCancel, 8, Side::Sell, 10400, 5),
        Order::new(OrderType::GoodTillCancel, 9, Side::Buy, 10500, 5),
        Order::new(OrderType::GoodTillCancel, 10, Side::Sell, 10500, 5),
    ];

    for o in orders {
        thread::sleep(std::time::Duration::from_secs(1));
        let bytes = bincode::serialize(&*o.lock().unwrap()).unwrap();
        write_frame(&mut stream, &bytes)?;

        let resp = read_frame(&mut stream)?;
        let msg: ServerMsg = bincode::deserialize(&resp).unwrap();
        eprintln!("CLIENT: server replied: {:?}", msg);
    }

    Ok(())
}