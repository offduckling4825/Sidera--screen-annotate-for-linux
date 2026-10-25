#!/usr/bin/env python3
"""Test the real bridge in isolation; never probe the user's running service.

The shared fixture checks XML contents, queue ordering/expiry, lifecycle,
whiteboard state, rejected requests and slow connections.
"""
from test_bridge_lifecycle import main

if __name__ == "__main__":
    raise SystemExit(main())
