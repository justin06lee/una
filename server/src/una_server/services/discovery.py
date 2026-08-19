"""mDNS advertisement of _una._tcp so clients on the LAN can auto-discover the server."""

from __future__ import annotations

import logging
import socket

log = logging.getLogger(__name__)


class Discovery:
    def __init__(self, port: int):
        self.port = port
        self._zc = None
        self._info = None

    def start(self) -> None:
        try:
            from zeroconf import ServiceInfo, Zeroconf

            hostname = socket.gethostname().split(".")[0]
            addresses = []
            try:
                probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                probe.connect(("8.8.8.8", 80))
                addresses = [socket.inet_aton(probe.getsockname()[0])]
                probe.close()
            except OSError:
                pass
            if not addresses:
                log.warning("mDNS disabled: could not determine LAN address")
                return
            self._info = ServiceInfo(
                "_una._tcp.local.",
                f"{hostname}._una._tcp.local.",
                port=self.port,
                addresses=addresses,
                properties={"version": "1", "api": "/v1"},
                # distinct from the machine's hostname: avahi usually owns
                # "<hostname>.local." and strict probing would see a conflict
                server=f"{hostname}-una.local.",
            )
            self._zc = Zeroconf()
            # cooperating_responders: tolerate avahi answering on the same host
            self._zc.register_service(
                self._info, allow_name_change=True, cooperating_responders=True
            )
            log.info("mDNS: advertising _una._tcp on port %d", self.port)
        except Exception as exc:
            log.warning("mDNS disabled: %r", exc, exc_info=True)

    def stop(self) -> None:
        if self._zc is not None:
            try:
                self._zc.unregister_service(self._info)
                self._zc.close()
            except Exception:
                pass
