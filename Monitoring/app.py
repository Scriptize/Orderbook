from nicegui import ui
import asyncio
import websockets
import json


WS_URL = "ws://127.0.0.1:9001"

@ui.page('/')
def page():
    q = asyncio.Queue()

    log = ui.column()

    # --- UI consumer ---
    async def consume():
        while True:
            msg = await q.get()

            with log:
                ui.label(str(msg))

    # --- WebSocket listener ---
    async def listen():
        while True:
            try:
                async with websockets.connect(WS_URL) as ws:
                    print("connected")

                    async for message in ws:
                        data = json.loads(message)
                        await q.put(data)

            except Exception as e:
                print("reconnecting...", e)
                await asyncio.sleep(1)

    asyncio.create_task(consume())
    asyncio.create_task(listen())

ui.run()





# import logging
# from datetime import datetime

# import random
# from scripts import mock_data, leos_func
# import plotly.graph_objects as go
# import numpy as np
# import asyncio
# import math
# from receiver import start_tcp_server, handle_client, handle_order, subscribers



# logger = logging.getLogger()

# class LogElementHandler(logging.Handler):
#     """A logging handler that emits messages to a log element."""

#     def __init__(self, element: ui.log, level: int = logging.NOTSET) -> None:
#         self.element = element
#         super().__init__(level)

#     def emit(self, record: logging.LogRecord) -> None:
#         try:
#             msg = self.format(record)
#             self.element.push(msg)
#         except Exception:
#             self.handleError(record)

# @ui.page('/')
# def page():
#     q = asyncio.Queue()
#     subscribers.add(q)
#     order_logs = ui.log().classes('w-25 h-25')

    
#     async def subscribe():
#         while True:
#             msg = await q.get()
#             print("Order Recieved!")
#             otype, oid, oside, oprice, oquantity, *_ = msg
#             order_logs.push(f"{oside}Order for {oquantity}/{oquantity} @ {oprice}", 
#                             classes = "text-red" if oside == "Sell" else "text-green")
            
#     asyncio.create_task(subscribe())
#     # match_log = ui.log()
#     # sys_log = ui.log()

#     # Initialize theme state
#     is_dark_mode = False

#     # Dark mode toggle button
#     dark_mode_button = ui.button('Toggle Dark Mode').on_click(lambda: toggle_dark_mode())

#     def toggle_dark_mode():
#         nonlocal is_dark_mode
#         is_dark_mode = not is_dark_mode  # Toggle the state
#         # Use JavaScript to change the CSS
#         ui.run_javascript(f"""
#             document.body.style.backgroundColor = '{'black' if is_dark_mode else 'white'}';
#             document.body.style.color = '{'white' if is_dark_mode else 'black'}';
#         """)
    

#     # Uncomment the following lines if you want to use logging handlers
#     # update_hdl = LogElementHandler(update_log)
#     # match_hdl = LogElementHandler(match_log)
#     # sys_hdl = LogElementHandler(sys_log)

#     # logger.addHandler(update_hdl)
#     # logger.addHandler(match_hdl)
#     # logger.addHandler(sys_hdl)

#     # ui.context.client.on_disconnect(lambda: logger.removeHandler(update_hdl))
#     # ui.context.client.on_disconnect(lambda: logger.removeHandler(match_hdl))
#     # ui.context.client.on_disconnect(lambda: logger.removeHandler(sys_hdl))

#     # ui.timer(random.randint(1, 2), lambda: leos_func(random.randint(1, 5), update_log=update_log, matches_log=match_log, systems_log=sys_log))

    
# @ui.page("/analytics")
# def analytics():
#     grid = ui.aggrid({
#     'defaultColDef': {'flex': 1},
#     'columnDefs': [
#         {'headerName': 'Name', 'field': 'name'},
#         {'headerName': 'Age', 'field': 'age'},
#         {'headerName': 'Parent', 'field': 'parent', 'hide': True},  
#     ],
#     'rowData': [
#         {'name': 'Alice', 'age': 18, 'parent': 'David'},
#         {'name': 'Bob', 'age': 21, 'parent': 'Eve'},
#         {'name': 'Carol', 'age': 42, 'parent': 'Frank'},
#     ],
#     'rowSelection': 'multiple',
#     }).classes('max-h-40')

#     def generate_order_book():
#         prices = np.linspace(98, 102, 200)
#         mid = 100
#         bids = np.maximum(0, 1500 - 300 * (mid - prices))
#         bids[prices > mid] = 0
#         asks = np.maximum(0, 300 * (prices - mid))
#         asks[prices < mid] = 0
#         return prices, bids, asks

#     # initial chart
#     prices, bids, asks = generate_order_book()
#     fig = go.Figure()
#     fig.add_trace(go.Scatter(x=prices, y=bids, mode='lines', name='Bids', line=dict(color='blue')))
#     fig.add_trace(go.Scatter(x=prices, y=asks, mode='lines', name='Asks', line=dict(color='orange')))
#     fig.update_layout(
#         title='Live Order Book Depth',
#         xaxis_title='Price',
#         yaxis_title='Quantity',
#         template='plotly_white',
#     )

#     chart = ui.plotly(fig).classes('w-[800px] h-[500px]')

#     async def update_chart():
#         while True:
#             prices, bids, asks = generate_order_book()
#             chart.figure.data[0].x = prices
#             chart.figure.data[0].y = bids
#             chart.figure.data[1].x = prices
#             chart.figure.data[1].y = asks
#             chart.update()
#             await asyncio.sleep(1)



# ui.timer(1.0, lambda: asyncio.create_task(start_tcp_server()), once=True)
# ui.run()
