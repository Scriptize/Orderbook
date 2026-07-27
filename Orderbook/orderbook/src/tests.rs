use super::*;
use std::collections::HashSet;

fn make_order(
    order_id: OrderId,
    order_type: OrderType,
    side: Side,
    price: Price,
    quantity: Quantity,
) -> Order {
    Order::new(0, order_type, order_id, side, price, quantity).unwrap()
}

fn assert_orderbook_invariants(orderbook: &Orderbook) {
    let mut queued_order_ids = HashSet::new();

    for (side, levels) in [(Side::Buy, &orderbook.bids), (Side::Sell, &orderbook.asks)] {
        for (price, level) in levels {
            assert_eq!(
                level.order_count,
                level.order_ids.len(),
                "incorrect count for {side:?} level at {price}"
            );

            let calculated_quantity: Quantity = level
                .order_ids
                .iter()
                .copied()
                .enumerate()
                .map(|(expected_location, order_id)| {
                    assert!(
                        queued_order_ids.insert(order_id),
                        "order {order_id} appears in multiple levels"
                    );

                    let order = orderbook
                        .orders
                        .get(&order_id)
                        .expect("queued order is missing from orders");

                    assert_eq!(order.get_side(), side);
                    assert_eq!(order.get_price(), *price);

                    let entry = orderbook
                        .order_index
                        .get(&order_id)
                        .expect("queued order is missing from order_index");

                    assert_eq!(entry.side, side);
                    assert_eq!(entry.price, *price);
                    assert_eq!(entry.location, expected_location);

                    order.get_remaining_quantity()
                })
                .sum();

            assert_eq!(
                level.total_quantity, calculated_quantity,
                "incorrect quantity for {side:?} level at {price}"
            );
        }
    }

    assert_eq!(queued_order_ids.len(), orderbook.orders.len());
    assert_eq!(queued_order_ids.len(), orderbook.order_index.len());

    for order_id in orderbook.orders.keys() {
        assert!(
            queued_order_ids.contains(order_id),
            "order {order_id} is not in a price level"
        );
    }

    for order_id in orderbook.order_index.keys() {
        assert!(
            queued_order_ids.contains(order_id),
            "indexed order {order_id} is not in a price level"
        );
    }
}

#[test]
fn test_orderbook_new() {
    let orderbook = Orderbook::new();

    assert_eq!(orderbook.size(), 0);
    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_price_level_push_updates_aggregates() {
    let mut level = PriceLevel::default();

    assert_eq!(level.push(1, 10), 0);
    assert_eq!(level.push(2, 20), 1);
    assert_eq!(level.push(3, 30), 2);

    assert_eq!(level.order_ids, vec![1, 2, 3]);
    assert_eq!(level.total_quantity, 60);
    assert_eq!(level.order_count, 3);
}

#[test]
fn test_price_level_remove_updates_aggregates() {
    let mut level = PriceLevel::default();

    level.push(1, 10);
    level.push(2, 20);
    level.push(3, 30);
    level.remove(1, 20);

    assert_eq!(level.order_ids, vec![1, 3]);
    assert_eq!(level.total_quantity, 40);
    assert_eq!(level.order_count, 2);
}

#[test]
fn test_orderbook_add_order_updates_level_aggregates() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 100, 20))
        .unwrap();

    orderbook
        .add_order(make_order(3, OrderType::GoodTillCancel, Side::Buy, 100, 30))
        .unwrap();

    let level = orderbook.bids.get(&100).unwrap();

    assert_eq!(orderbook.size(), 3);
    assert_eq!(level.order_ids, vec![1, 2, 3]);
    assert_eq!(level.total_quantity, 60);
    assert_eq!(level.order_count, 3);

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_cancel_middle_order_repairs_indices() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 100, 20))
        .unwrap();

    orderbook
        .add_order(make_order(3, OrderType::GoodTillCancel, Side::Buy, 100, 30))
        .unwrap();

    orderbook.cancel_order(2).unwrap();

    let level = orderbook.bids.get(&100).unwrap();

    assert_eq!(level.order_ids, vec![1, 3]);
    assert_eq!(level.total_quantity, 40);
    assert_eq!(level.order_count, 2);
    assert_eq!(orderbook.order_index.get(&1).unwrap().location, 0);
    assert_eq!(orderbook.order_index.get(&3).unwrap().location, 1);
    assert!(!orderbook.orders.contains_key(&2));
    assert!(!orderbook.order_index.contains_key(&2));

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_cancel_final_order_removes_price_level() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook.cancel_order(1).unwrap();

    assert!(!orderbook.bids.contains_key(&100));
    assert!(orderbook.orders.is_empty());
    assert!(orderbook.order_index.is_empty());

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_orderbook_cancel_order() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook
        .add_order(make_order(3, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook.cancel_order(1).unwrap();
    assert_orderbook_invariants(&orderbook);

    orderbook.cancel_order(2).unwrap();
    assert_orderbook_invariants(&orderbook);

    orderbook.cancel_order(3).unwrap();

    assert_eq!(orderbook.size(), 0);
    assert!(orderbook.bids.is_empty());
    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_modify_order_moves_order_to_new_level() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook
        .modify_order(OrderModify::new(0, 1, Side::Buy, 101, 7))
        .unwrap();

    assert!(!orderbook.bids.contains_key(&100));

    let level = orderbook.bids.get(&101).unwrap();

    assert_eq!(level.order_ids, vec![1]);
    assert_eq!(level.total_quantity, 7);
    assert_eq!(level.order_count, 1);

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_order_modify_order_matches_after_side_change() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    let result = orderbook
        .modify_order(OrderModify::new(0, 2, Side::Sell, 100, 10))
        .unwrap();

    assert_eq!(result.get_trades().len(), 1);
    assert_eq!(orderbook.size(), 0);
    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_orderbook_wont_match_non_crossing_orders() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    let result = orderbook
        .add_order(make_order(
            2,
            OrderType::GoodTillCancel,
            Side::Sell,
            101,
            10,
        ))
        .unwrap();

    assert!(result.get_trades().is_empty());
    assert_eq!(orderbook.size(), 2);
    assert!(orderbook.bids.contains_key(&100));
    assert!(orderbook.asks.contains_key(&101));

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_orderbook_wont_match_same_side_orders() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    let result = orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    assert!(result.get_trades().is_empty());
    assert_eq!(orderbook.size(), 2);
    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_orderbook_can_match() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Sell, 100, 1))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 80, 1))
        .unwrap();

    let result = orderbook
        .add_order(make_order(3, OrderType::GoodTillCancel, Side::Buy, 105, 1))
        .unwrap();

    assert_eq!(result.get_trades().len(), 1);
    assert_eq!(orderbook.size(), 1);
    assert_eq!(*orderbook.bids.first_key_value().unwrap().0, 80);
    assert!(orderbook.asks.is_empty());
    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_partial_fill_updates_aggregates_at_different_prices() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 105, 10))
        .unwrap();

    let result = orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Sell, 100, 4))
        .unwrap();

    assert_eq!(result.get_trades().len(), 1);

    let bid_level = orderbook.bids.get(&105).unwrap();

    assert_eq!(bid_level.order_ids, vec![1]);
    assert_eq!(bid_level.total_quantity, 6);
    assert_eq!(bid_level.order_count, 1);
    assert!(!orderbook.asks.contains_key(&100));
    assert_eq!(
        orderbook.orders.get(&1).unwrap().get_remaining_quantity(),
        6
    );
    assert!(!orderbook.orders.contains_key(&2));

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_full_fill_removes_both_levels() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 105, 10))
        .unwrap();

    let result = orderbook
        .add_order(make_order(
            2,
            OrderType::GoodTillCancel,
            Side::Sell,
            100,
            10,
        ))
        .unwrap();

    assert_eq!(result.get_trades().len(), 1);
    assert!(orderbook.bids.is_empty());
    assert!(orderbook.asks.is_empty());
    assert!(orderbook.orders.is_empty());
    assert!(orderbook.order_index.is_empty());

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_can_fully_fill_buy_across_multiple_ask_levels() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Sell, 100, 4))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Sell, 101, 6))
        .unwrap();

    orderbook
        .add_order(make_order(
            3,
            OrderType::GoodTillCancel,
            Side::Sell,
            102,
            100,
        ))
        .unwrap();

    assert!(orderbook.can_fully_fill(Side::Buy, 101, 10));
    assert!(!orderbook.can_fully_fill(Side::Buy, 101, 11));
    assert!(!orderbook.can_fully_fill(Side::Buy, 99, 1));

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_can_fully_fill_sell_across_multiple_bid_levels() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 102, 4))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 101, 6))
        .unwrap();

    orderbook
        .add_order(make_order(
            3,
            OrderType::GoodTillCancel,
            Side::Buy,
            100,
            100,
        ))
        .unwrap();

    assert!(orderbook.can_fully_fill(Side::Sell, 101, 10));
    assert!(!orderbook.can_fully_fill(Side::Sell, 101, 11));
    assert!(!orderbook.can_fully_fill(Side::Sell, 103, 1));

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_fill_or_kill_fills_across_multiple_levels() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Sell, 100, 4))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Sell, 101, 6))
        .unwrap();

    let result = orderbook
        .add_order(make_order(3, OrderType::FillOrKill, Side::Buy, 101, 10))
        .unwrap();

    assert_eq!(result.get_trades().len(), 2);
    assert!(orderbook.bids.is_empty());
    assert!(orderbook.asks.is_empty());
    assert!(orderbook.orders.is_empty());

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_rejected_fill_or_kill_does_not_mutate_book() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Sell, 100, 4))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Sell, 101, 6))
        .unwrap();

    let result = orderbook.add_order(make_order(3, OrderType::FillOrKill, Side::Buy, 101, 11));

    assert!(matches!(result, Err(OrderbookError::FillOrKillNotFillable)));
    assert_eq!(orderbook.size(), 2);
    assert_eq!(orderbook.asks.get(&100).unwrap().total_quantity, 4);
    assert_eq!(orderbook.asks.get(&101).unwrap().total_quantity, 6);
    assert!(!orderbook.orders.contains_key(&3));

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_partially_filled_fill_and_kill_removes_remainder() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Sell, 100, 4))
        .unwrap();

    let result = orderbook
        .add_order(make_order(2, OrderType::FillAndKill, Side::Buy, 100, 10))
        .unwrap();

    assert_eq!(result.get_trades().len(), 1);
    assert!(orderbook.bids.is_empty());
    assert!(orderbook.asks.is_empty());
    assert!(orderbook.orders.is_empty());
    assert!(orderbook.order_index.is_empty());

    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_orderbook_will_cancel_unmatched_fill_and_kill() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 250, 5))
        .unwrap();

    let result = orderbook.add_order(make_order(2, OrderType::FillAndKill, Side::Buy, 100, 10));

    assert!(matches!(
        result,
        Err(OrderbookError::FillAndKillNotMatchable)
    ));
    assert_eq!(orderbook.size(), 1);
    assert!(!orderbook.orders.contains_key(&2));
    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_orderbook_will_cancel_fok_when_liquidity_is_insufficient() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Sell, 100, 5))
        .unwrap();

    let result = orderbook.add_order(make_order(2, OrderType::FillOrKill, Side::Buy, 100, 10));

    assert!(matches!(result, Err(OrderbookError::FillOrKillNotFillable)));
    assert_eq!(orderbook.size(), 1);
    assert!(!orderbook.orders.contains_key(&2));
    assert_orderbook_invariants(&orderbook);
}

#[test]
fn test_snapshot_reports_level_aggregates() {
    let mut orderbook = Orderbook::new();

    orderbook
        .add_order(make_order(1, OrderType::GoodTillCancel, Side::Buy, 100, 10))
        .unwrap();

    orderbook
        .add_order(make_order(2, OrderType::GoodTillCancel, Side::Buy, 100, 20))
        .unwrap();

    orderbook
        .add_order(make_order(
            3,
            OrderType::GoodTillCancel,
            Side::Sell,
            101,
            15,
        ))
        .unwrap();

    let snapshot = orderbook.get_order_infos();

    assert_eq!(snapshot.get_bids().len(), 1);
    assert_eq!(snapshot.get_bids()[0].price, 100);
    assert_eq!(snapshot.get_bids()[0].quantity, 30);
    assert_eq!(snapshot.get_asks().len(), 1);
    assert_eq!(snapshot.get_asks()[0].price, 101);
    assert_eq!(snapshot.get_asks()[0].quantity, 15);

    assert_orderbook_invariants(&orderbook);
}

// #[test]
// fn test_add_market_order() -> Result<(), Box<dyn std::error::Error>> {
//     let mut orderbook = Orderbook::new();
//
//     orderbook.add_order(Order::new(
//         0,
//         OrderType::GoodTillCancel,
//         1,
//         Side::Buy,
//         100,
//         10,
//     )?)?;
//
//     orderbook.add_order(Order::new(
//         0,
//         OrderType::GoodTillCancel,
//         2,
//         Side::Buy,
//         150,
//         10,
//     )?)?;
//
//     orderbook.add_order(Order::new(
//         0,
//         OrderType::GoodTillCancel,
//         3,
//         Side::Sell,
//         200,
//         10,
//     )?)?;
//
//     orderbook.add_order(Order::new(
//         0,
//         OrderType::GoodTillCancel,
//         4,
//         Side::Sell,
//         300,
//         10,
//     )?)?;
//
//     orderbook.add_order(Order::new_market(
//         0,
//         5,
//         Side::Buy,
//         10,
//     )?)?;
//
//     let level_infos = orderbook.get_order_infos();
//     let asks = level_infos.get_asks();
//
//     assert_eq!(asks.len(), 1);
//     assert_eq!(asks[0].price, 200);
//     assert_eq!(asks[0].quantity, 10);
//
//     assert_orderbook_invariants(&orderbook);
//
//     Ok(())
// }
