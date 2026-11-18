"""Blip - a little office pet for your Windows desktop."""
try:
    from ._build import VERSION as __version__
except ImportError:
    __version__ = "0.0.0-dev"
