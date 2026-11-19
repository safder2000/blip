"""Old name for the Blip server, kept so `python -m blip.relay` still works. See blip/server.py."""
from .server import DEFAULT_PORT, Server, main  # noqa: F401

Relay = Server

if __name__ == "__main__":
    main()
