# Using una away from home

una's server lives on your GPU box. On your home network the client reaches it
at a LAN address; from a café, a hotel, or a phone tether that address means
nothing. This is how to make one setup work in both places.

## The shape of it

The client holds a **list** of server addresses and, before each dictation,
uses whichever one answers first. Nothing to switch when you move:

```toml
[server]
urls = [
    "http://192.168.1.253:8100",              # LAN — fastest at home
    "http://box.tailnet-name.ts.net:8100",    # VPN — works anywhere
]
autodiscover = true
```

All addresses are probed **at the same time**, so an address that is dead right
now costs nothing — the reachable one still wins in well under a second. The
winner is cached, so the check does not add latency to every dictation; it is
re-probed when a dictation fails or after five minutes.

You can edit the list in **Settings → Server**. **Test all** probes each address
from wherever you are and shows which ones answer, with round-trip times — the
quickest way to confirm remote access before you actually need it.

## ⚠️ Do not simply port-forward

una has **no authentication**. Anyone who can reach port 8100 can read every
recording and transcript you have ever made, and can swap the model that serves
you. Forwarding it through your router puts all of that on the public internet.

Use a private network instead. The rest of this page assumes Tailscale, which is
free for personal use, needs no open ports, and encrypts everything end to end.
WireGuard or a self-hosted equivalent works the same way.

## Tailscale setup

1. **Install it on the server and on every client** you dictate from:
   <https://tailscale.com/download>. Sign both into the same tailnet.

   On Arch, as on the una server box:

   ```sh
   sudo pacman -S tailscale
   sudo systemctl enable --now tailscaled
   sudo tailscale up
   ```

   `enable` matters — without it the machine drops off the tailnet after a
   reboot and remote dictation stops working.

2. **Find the server's tailnet address**, on the server:

   ```sh
   tailscale status --json | grep -i dnsname   # e.g. box.tailnet-name.ts.net
   tailscale ip -4                             # e.g. 100.x.y.z
   ```

3. **Add it to the client**, in Settings → Server → *Add address*:
   `http://box.tailnet-name.ts.net:8100`. Keep the LAN address above it so home
   dictation still takes the fast path. Adding the raw `100.x.y.z` address as a
   third entry is a cheap insurance policy for the day MagicDNS misbehaves.

4. **Turn off key expiry for the server.** This is the step people miss.
   Tailscale node keys expire (180 days by default); when the server's key
   expires it silently leaves the tailnet and remote dictation stops until
   someone re-authenticates at its console. In the admin console
   (<https://login.tailscale.com/admin/machines>) open the server's **⋯** menu
   and choose **Disable key expiry**.

5. **Check the firewall** lets the tailnet in. On a firewalld box the clean way
   is to trust the interface rather than opening the port to the world:

   ```sh
   sudo firewall-cmd --permanent --zone=trusted --add-interface=tailscale0
   sudo firewall-cmd --reload
   ```

Then, from anywhere, `http://box.tailnet-name.ts.net:8100` is the dashboard and
the client's dictation endpoint.

## Keeping the server awake

Remote access is worthless if the box is asleep. On the server:

```sh
sudo systemctl mask sleep.target suspend.target hibernate.target hybrid-sleep.target
systemctl is-enabled una tailscaled     # both should say "enabled"
```

## What to expect over a remote link

- **Latency.** Dictation uploads a WAV, so the round trip is dominated by your
  uplink. A 10-second utterance is roughly 320 kB; on a decent connection that
  is a fraction of a second on top of the usual transcription time.
- **Tailscale prefers a direct path** and falls back to a relay when it cannot
  punch through. `tailscale ping <server>` tells you which you got — `via
  <ip>:port` is direct, `via DERP` is relayed and slower. The first packet after
  idling is often relayed while the direct path is re-established; that is
  normal and does not mean anything is wrong.
- **A failed upload is not lost.** The client spools the audio and the tray's
  *Retry Last Dictation* re-sends it once you have signal again.
- **Roaming mid-session** costs one extra round trip: the first dictation after
  you change networks fails against the cached address, the client immediately
  re-probes and re-sends the same utterance to the address that now works. The
  server dedupes on the utterance id, so a retry can never produce a duplicate.

## Troubleshooting

| Symptom | Check |
|---|---|
| Works at home, not away | Settings → Server → **Test all**. If only the LAN address is green, the VPN address is missing or the VPN is down on this machine. |
| Nothing is reachable | Is the VPN connected on *this* device? `tailscale status` should list the server as online. |
| Server missing from the tailnet | Its node key probably expired — see step 4 above. |
| Reachable but slow | `tailscale ping <server>`; if it says DERP you are being relayed. |
| Reachable, dictation still fails | The dashboard's status card shows whether the ASR model is loaded and whether a training run has the GPU. |
