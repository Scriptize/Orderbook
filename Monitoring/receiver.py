import asyncio
import struct
import msgpack
from nicegui import ui


subscribers = set()
# Helper functions to append to the UI


# TCP message handlers

async def handle_order(reader):
    header = await reader.readexactly(4)
    frame_len = int.from_bytes(header, 'big')
    data = await reader.readexactly(frame_len)
    order = msgpack.loads(data)
    await publish(order)
    # otype, oid, oside, oprice, oquantity, *_ = order
    # print("recieved", order)
    # append_log(f"{oside}Order for {oquantity}/{oquantity} @ {oprice}")
    
async def publish(message):
    for q in list(subscribers):
        await q.put(message)


# TCP connection handler
async def handle_client(reader, writer):
    addr = writer.get_extra_info('peername')
    print(f"[STATUS] Connected byx` {addr}")
    try:
        while True:
            await handle_order(reader)
    except asyncio.IncompleteReadError:
        print(f"[STATUS] Connection from {addr} closed")
    except Exception as e:
        print(f"[ERROR] {e}")
    finally:
        writer.close()
        await writer.wait_closed()

# Async TCP server startup
async def start_tcp_server():
    server = await asyncio.start_server(handle_client, 'localhost', 9000)
    print("[STATUS] TCP server started on port 9000")
    asyncio.create_task(server.serve_forever())


# # Use timer to start server after NiceGUI boots
# ui.timer(1.0, lambda: asyncio.create_task(start_tcp_server()), once=True)

# # Run the app
# ui.run()
