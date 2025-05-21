from nicegui import ui
import asyncio
import random
import time

mock_data = [
 {"type":"price_update","symbol":"BTC/USD","old_price":67203.00,"new_price":67210.50},
 {"type":"price_update","symbol":"ETH/USD","old_price":3212.45,"new_price":3217.10},
 {"type":"price_update","symbol":"SPY","old_price":528.75,"new_price":529.00},
 {"type":"match","side":"BUY","quantity":100,"symbol":"AAPL","price":172.34},
 {"type":"match","side":"SELL","quantity":5,"symbol":"BTC","price":67200.00},
 {"type":"match","side":"BUY","quantity":50,"symbol":"TSLA","price":187.90},
 {"type":"log","level":"INFO","message":"TCP server started on port 9001"},
 {"type":"log","level":"DEBUG","message":"Received order: ID#14352 (BUY 25 ETH @ $3,200)"},
 {"type":"log","level":"INFO","message":"Trade executed: Order#14352 matched with Order#14349"}
]

update_log = ui.log(max_lines=10).classes('w-full h-20')
matches_log = ui.log(max_lines=10).classes('w-full h-20')
systems_log = ui.log(max_lines=10).classes('w-full h-20')

async def leos_func(num_of_logs, update_log, matches_log, systems_log):
    for _ in range(num_of_logs):
        random_log_event = random.choice(mock_data)
        if random_log_event["type"] == "log":
            systems_log.push(random_log_event)
        elif random_log_event["type"] == "match":
            matches_log.push(random_log_event)
        else:
            update_log.push(random_log_event)

    

# while True: #connected to server
#     interval = random.randint(7)
#     log_num = random.randint(5)
#     asyncio.create_task(leos_func(log_num))
#     time.sleep(interval)


# async def simulated_callback():
#     counter = 0
#     while True:
#         await asyncio.sleep(2)
#         log.push(f"[INFO] Log line {counter}") # replace with an actual log object
#         counter += 1

# asyncio.create_task(simulated_callback())


# ##############
# @ui.page('/')
# def page():
#     log1 = ui.log(max_lines=10).classes('w-full')
#     log2 = ...
#     log3 = ...
#     handler = LogElementHandler(log) #can copy
#     logger.addHandler(handler) #can copy
#     ui.context.client.on_disconnect(lambda: logger.removeHandler(handler)) #can copy
#     ui.timer(random(7), leos_func(random(5))) # every 1-7 secs, handle 1-5 logs for l1, l2, l3