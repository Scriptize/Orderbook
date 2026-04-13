use orderbook::OrderbookLevelInfos;
use exchange::Command;
use exchange::Event

trait Actor {
    fn step(&mut self, book: OrderbookLevelInfos) -> Vec<Command>;
    fn on_event(&mut self, event: &Event);
}


struct MarketMaker {

}

struct NoiseTrader {

} 

struct Taker {

}

struct InformedTrader {

}

struct LiquiditySweeper {

}
