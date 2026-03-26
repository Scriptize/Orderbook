mod orderbook;
use std::{
    cell::RefCell, collections::{BTreeMap, HashMap}, rc::Rc, time::Instant
};
use crate::orderbook::{Orderbook, Order, OrderType, Side};
use log::{info, warn, error, debug, trace};
use std::thread;
use std::time::Duration;
use colored::*;
use fern::Dispatch;
use std::sync::{Arc, Mutex};




// Removed init_logging as we only use setup_logger for logging initialization
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
    let mut orderbook  = Orderbook::new(BTreeMap::new(), BTreeMap::new());
    let book = Arc::new(Mutex::new(orderbook));
    start_pruning_thread(book.clone());

    for i in 2001..=2010 {
        let mut order = Order::new(
            OrderType::GoodForDay,
            i,
            Side::Buy,
            50u64.try_into().unwrap(),
            1000 + (i % 100) * 100,
        );
        order.set_expiry(Instant::now() + Duration::from_secs(5));

        {
            let mut ob = book.lock().unwrap();
            ob.add_order(order);
        }
        
    }
    
    for i in 1..=1000 {
        let mut order = Order::new(
            if i % 2 == 0 { OrderType::GoodTillCancel } else { OrderType::Market },
            i,
            if i%100 == 0 {Side::Sell} else {Side::Buy},
            (100 + i as u64).try_into().unwrap(), // price increases with i
            5 + (i % 10), // varying quantity
        );
        {
            let mut ob = book.lock().unwrap();
            ob.add_order(order);
        }
        
    }

    for i in 1001..=2000 {
    
        let mut order = Order::new(
            if i % 2 == 0 { OrderType::GoodTillCancel } else { OrderType::FillOrKill },
            i,
            Side::Sell,
            (110 - (i % 20) as u64).try_into().unwrap(), // price decreases with i, some overlap with buys
            3 + (i % 7), // varying quantity
            
        );
        {
            let mut ob = book.lock().unwrap();
            ob.add_order(order);
        }
        
        thread::sleep(Duration::from_millis(10));
    
    }

    println!("Main thread complete.");
    
}

pub fn start_pruning_thread(book: Arc<Mutex<Orderbook>>){
            thread::spawn(move || {
                loop{
                    thread::sleep(Duration::from_secs(1));

                    let mut ob = book.lock().unwrap();
                    ob.prune_expired();
                }
            });
        }
