import asyncio
import struct
import bincode
from nicegui import ui

# Create a column to display log/match messages
with ui.column().classes('w-full') as log_column:
    ui.label('TCP Server Logs:')

# Label to show price updates
match_label = ui.label('Price updates will show here').classes('text-xl')

# UI label for server status
ui.label('NiceGUI TCP Server is running...')

# Helper functions to append to the UI
def append_log(text: str):
    with log_column:
        ui.label(text)

# TCP message handlers
async def handle_log(reader):
    header = await reader.readexactly(4)
    len_level, len_msg = struct.unpack("!HH", header)
    payload = await reader.readexactly(len_level + len_msg)
    level = payload[:len_level].decode()
    message = payload[len_level:].decode()
    append_log(f"[LOG] {level}: {message}")

async def handle_match(reader):
    header = await reader.readexactly(12)
    len_side, len_symbol, quantity, price = struct.unpack("!HHIf", header)
    payload = await reader.readexactly(len_side + len_symbol)
    side = payload[:len_side].decode()
    symbol = payload[len_side:].decode()
    append_log(f"[MATCH] {side} {quantity} {symbol} @ ${price:.2f}")

async def handle_price_update(reader):
    header = await reader.readexactly(10)
    len_symbol, old_price, new_price = struct.unpack("!Hff", header)
    symbol = (await reader.readexactly(len_symbol)).decode()
    match_label.set_text(f"[PRICE] {symbol}: ${old_price:.2f} -> ${new_price:.2f}")

async def handle_order(reader):
    header = await reader.readexactly(4)
    frame_len = int.from_bytes(header, 'big')
    data = await reader.readexactly(frame_len)
    order = bincode.loads(data)
    print(order)


# TCP connection handler
async def handle_client(reader, writer):
    addr = writer.get_extra_info('peername')
    append_log(f"[STATUS] Connected by {addr}")
    try:
        while True:
            await handle_order(reader)
            # msg_type_raw = await reader.read(1)
            # if not msg_type_raw:
            #     break
            # msg_type = struct.unpack("!B", msg_type_raw)[0]

            # if msg_type == 1:
            #     await handle_log(reader)
            # elif msg_type == 2:
            #     await handle_match(reader)
            # elif msg_type == 3:
            #     await handle_price_update(reader)
            # else:
            #     append_log(f"[ERROR] Unknown message type: {msg_type}")
            #     break
    except asyncio.IncompleteReadError:
        append_log(f"[STATUS] Connection from {addr} closed")
    except Exception as e:
        append_log(f"[ERROR] {e}")
    finally:
        writer.close()
        await writer.wait_closed()

# Async TCP server startup
async def start_tcp_server():
    server = await asyncio.start_server(handle_client, 'localhost', 12345)
    append_log("[STATUS] TCP server started on port 12345")
    asyncio.create_task(server.serve_forever())

# Use timer to start server after NiceGUI boots
ui.timer(1.0, lambda: asyncio.create_task(start_tcp_server()), once=True)

# Run the app
ui.run()
