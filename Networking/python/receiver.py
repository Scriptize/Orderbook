import socket
import struct
import random
import math

def main():
    print("This will be our python reciever! (server)")

    # format_char_size = { me when i dont know theres a calcsize()
    #     "X": 0,
    #     "c": 1,
    #     "b": 1,
    #     "B": 1,
    #     "?": 1,
    #     "h": 2,
    #     "H": 2,
    #     "i": 4,
    #     "I": 4,
    #     "l": 4,
    #     "L": 4,
    #     "q": 8,
    #     "Q": 8,
    #     "n": 0,
    #     "N": 0,
    #     "e": 2,
    #     "f": 4,
    #     "d": 8,
    #     "s": 0,
    #     "p": 0,
    #     "P": 0
    # }
    
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        # creates/binds to a TCP stream
        try:
            s.bind(('localhost', 12345))
        except socket.error:
            print("Error: Cannot bind to a TCP stream.")
        
        s.listen(1) # 1 pending connections that the socket can queue
        print("Server is listening...")

        # accepts a TCP connection
        conn, addr = s.accept()
        with conn:
            print('Connected by', addr)
            while True:
                log_format_str = "!B3s"
                match_format_str = "!Bssisf"
                price_update_format_str = "!Bssff"

                log_buffer_size = struct.calcsize(log_format_str)
                match_buffer_size = struct.calcsize(match_format_str)
                price_update_buffer_size = struct.calcsize(price_update_format_str)

                buffer_size = max(log_buffer_size, match_buffer_size, price_update_buffer_size)

                # unpacks a structured buffer
                buffer = conn.recv(buffer_size)
                # could it recieve more than 1 "buffer" since we may be recieving more than the size of 1 buffer? 
                if not buffer: break

                log_type_int = buffer.unpack("!B", buffer)
                # the data variable is a collection of elements
                if log_type_int == 1:
                    data = struct.unpack_from("!3s", buffer, 1) # offset packing by 1 to not unpack the identifier type
                    
                    # data = (log_type_str, level, message)
                    print(f"Log type: {data[0]}, level: {data[1]}, message: {data[2]}")
                    # reads the data (standard output, print, etc.)

                elif log_type_int == 2:
                    data = struct.unpack_from("!ssisf", buffer, 1)
                    
                    # data = (log_type_str, side, quantity, symbol, price)
                    print(f"Log type: {data[0]}, side: {data[1]}, quantity: {data[2]}, symbol: {data[3]}, price: {data[4]}")
                    # reads the data (standard output, print, etc.)
                elif log_type_int == 3:
                    data = struct.unpack_from("!ssff", buffer, 1)

                    # data = (log_type_str, symbol, old_price, new_price)
                    print(f"Log type: {data[0]}, symbol: {data[1]}, old_price: {data[2]}, new_price: {data[3]}")
                    # reads the data (standard output, print, etc.)
                else:
                    print("Error: log type is invalid.")

main()



