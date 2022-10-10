import socket
import sys
import time
from datetime import timedelta
import proto_schema.schema_pb2 as schema

# Create a UDS socket
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)

# Connect the socket to the port where the server is listening
server_address = "/tmp/pico.sock"
print("connecting to {}".format(server_address))
try:
    sock.connect(server_address)
except socket.error as msg:
    print(msg)
    sys.exit(1)

# Make a new pico message
pico = schema.picoMessage()
pico.audio.audioFile = "song3.mp3"


try:
    # Send data
    message = pico.SerializeToString()
    start_time = time.monotonic()
    sock.sendall(message)

finally:
    print("closing socket")
    sock.close()
