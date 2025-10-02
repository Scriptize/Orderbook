use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use std::thread;
use std::collections::{BTreeMap, HashMap};

use log::{info, warn, error, debug, trace};
use std::time::Duration;
use colored::*;
use fern::Dispatch;

use orderbook::orderbook::{Orderbook, Order, OrderType, Side};

struct Exchange {
    orderbook: Arc<Mutex<Orderbook>>,
}

#[derive(Serialize, Deserialize, Debug)]
enum ServerMsg {
    Ack { id: u32 },
    Err(String),
}

impl Exchange {
    fn new(orderbook: Orderbook) -> Self {
        Self { orderbook: Arc::new(Mutex::new(orderbook)) }
    }

    fn start(&self, addr: &str) {
        let listener = TcpListener::bind(addr).unwrap();
        println!("Exchange listening on {}", addr);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let ob = Arc::clone(&self.orderbook);
                    thread::spawn(move || {
                        Exchange::handle_client(stream, ob);
                    });
                }
                Err(e) => eprintln!("Connection failed: {}", e),
            }
        }
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

    fn handle_client(mut stream: TcpStream, orderbook: Arc<Mutex<Orderbook>>) -> std::io::Result<()> {
        loop {
            let frame = match Self::read_frame(&mut stream) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            };

            let order: Arc<Mutex<Order>> = match bincode::deserialize(&frame) {
                Ok(o) => Arc::new(Mutex::new(o)),
                Err(e) => {
                    let msg = ServerMsg::Err(format!("decode error: {e}"));
                    let bytes = bincode::serialize(&msg).unwrap();
                    Self::write_frame(&mut stream, &bytes)?;
                    continue;
                }
            };

            // do something with the order (e.g., push to orderbook)

            eprintln!("SERVER: received order: {:?}", order);
            let order_id = 
            {
                let book = orderbook.lock().unwrap();
                // let order = order.lock().unwrap();
                book.add_order(order.clone());
                order.lock().unwrap().get_order_id()
            };

            let msg = ServerMsg::Ack { id: order_id };
            let bytes = bincode::serialize(&msg).unwrap();
            Self::write_frame(&mut stream, &bytes)?;
        }
        Ok(())
    }
}
fn setup_logger() -> Result<(), Box<dyn std::error::Error>> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            let color_message = match record.level() {
                log::Level::Error => message.to_string().red().to_string(),
                log::Level::Warn => message.to_string().yellow().to_string(),
                log::Level::Info => message.to_string().green().to_string(),
                log::Level::Debug => message.to_string().blue().to_string(),
                log::Level::Trace => message.to_string().magenta().to_string(),
            };
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d %H:%M:%S:%.3f]"),
                record.target(),
                record.level(),
                color_message
            ))
        })
        .level(log::LevelFilter::Trace)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}

fn main() {
    setup_logger().unwrap();
    let exchange = Exchange::new(Orderbook::new(BTreeMap::new(), BTreeMap::new()));
    exchange.start("127.0.0.1:9000");
}

