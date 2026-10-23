# una on the iPhone — design note

Status: **design, no code yet** (2026-10-20). A native Swift app plus a custom keyboard
extension that dictates through the same una server the desktop client uses. It starts
free-sideloaded from a personal team; the $99 developer account comes later. One question
is left open on purpose, for you to decide: how the phone reaches una on tenet when you are
away from home ([section 4](#4-reaching-tenet-away-from-home)).

The code would live in `client/apps/ios/`, beside `client/apps/desktop/`.

## 1. Free sideloading

A free Apple ID in Xcode gives you a **Personal Team**. You build from the Mac straight onto
the phone, with a cable or over Wi-Fi once the two are paired.

### What a free personal team allows

| Limit | Free personal team | Source |
|---|---|---|
| Profile lifetime | **7 days**, then the app won't launch until you rebuild and reinstall from Xcode. Reinstalling over the same bundle id should keep its data (it installs as an update); deleting the app loses it. | [compare memberships](https://developer.apple.com/support/compare-memberships/) |
| Apps | 3 per device | same |
| Devices | 3 | same |
| App IDs | 10, each expiring after 7 days. **Every extension needs its own**: app + keyboard is 2 a week, and adding a widget or a Live Activity makes 3. Renaming bundle ids burns more. | same, [AltStore FAQ](https://faq.altstore.io/altstore-classic/app-ids) |
| Developer Mode | Must be on: Settings → Privacy & Security → Developer Mode, then restart. | [Apple](https://developer.apple.com/documentation/xcode/enabling-developer-mode-on-a-device) |

Whether the keyboard extension also takes one of the three app slots is unverified. una
would use at most two either way.

### Capabilities

From Apple's [supported-capabilities table](https://developer.apple.com/help/account/reference/supported-capabilities-ios),
where "Apple Developer" means a free account:

| Capability | Free | What una needs it for |
|---|---|---|
| **App Groups** | ✅ | The container shared by the app and the keyboard. Required. |
| **Background Modes** (audio) | ✅ | Keeping the mic session alive after you return to the host app. Required. |
| **Keychain Sharing** | ✅ | A token or client certificate readable by both targets, if section 4 needs one. |
| **Network Extensions** (packet tunnel, Personal VPN) | ❌ paid only | Only for a una- or makima-owned VPN (option 1a). Xcode refuses it outright: "Personal development teams … do not support the Network Extensions capability." |
| Push Notifications, Associated Domains, iCloud, Siri, Access Wi-Fi Information | ❌ paid only | Nothing in the plan needs them. |
| Multicast networking | ❌ requested by a paid team | Not needed: browsing a *declared* Bonjour type (`_una._tcp`) works without it. |

So everything the app and the keyboard need is available free. What the free team can't do is
carry its own VPN.

### What the $99 account changes

- **Profiles last a year**, not a week, and there's no weekly reinstall.
- **TestFlight**: builds install and update over the air. Internal testers skip review.
- **100 devices** per product family, per membership year.
- **Network Extension** providers, packet tunnel included, are self-serve: you tick the
  capability, with no request to Apple. That makes option 1a in section 4 possible.
- **The multicast entitlement** can be requested. una doesn't need it.
- **The App Store**, if you ever want it. Two review rules then bite:
  - Rule 4.4.1: a keyboard must "provide keyboard input functionality" and work without
    Full Access, so a mic-only pad would need typing keys.
  - Plain-HTTP exceptions need a justification.

Nothing in steps 1–3 of the build plan changes with the paid account; only the chores do.

## 2. The keyboard extension

### The microphone is off-limits to the keyboard

**A keyboard extension cannot record.** Apple's extension guide says custom keyboards "have
no access to the device microphone", and QA1872 says app extensions "are not allowed to
record audio". It is still true with Full Access on. Developers on Apple's forums get "NOT
allowed to start recording because it is an extension" (2023), and `AVAudioEngine` fails
with `!rec` (2025, filed as FB16791704, unfixed). Nothing in iOS 18 or 26 changed it.

Every shipping dictation keyboard gets around it the same way: **the keyboard opens its
containing app, and the app records.** Wispr Flow, Superwhisper and Willow all do this, and
Gboard did it for voice typing.

1. **Opening the app.** Keyboards can't use `extensionContext.open`. The working method walks
   the responder chain to the `UIApplication` and calls `open(_:options:completionHandler:)`.
   - iOS 18 broke the older `openURL:` selector trick.
   - On **iOS 26 the call only works with Full Access on**. Apple says that change is
     deliberate.
   - Apple DTS confirmed in January 2026 that App Review allows opening your *own*
     containing app from a keyboard (rule 4.4.1).
   - Sources: [forum 812091](https://developer.apple.com/forums/thread/812091),
     [forum 764570](https://developer.apple.com/forums/thread/764570).
2. **Recording in the background.** The app must *start* recording while it is in the
   foreground. The `audio` background mode then keeps it running after you leave. No time
   limit is documented while the session stays active.
3. **Getting back.** **There is no API to return to the host app.**
   - iOS 26.4 removed the lookup keyboards used to learn which app they were in, which also
     ended automatic return. Apple confirmed in June 2026 that no public API exists
     (FB22247647).
   - You tap the system **◀ back** link in the top-left corner, or swipe.
   - Superwhisper and Wispr Flow now tell their users the same.
   - Sources: [forum 826851](https://developer.apple.com/forums/thread/826851),
     [Superwhisper](https://ai.superwhisper.com/docs/common-issues/ios-26-keyboard-changes),
     [Wispr Flow](https://docs.wisprflow.ai/articles/7453988911-set-up-the-flow-keyboard-on-iphone).
4. **Sessions.** Recording can't restart from the background, so the app keeps the mic
   session open between dictations.
   - Wispr Flow's idle timeout offers *Immediately*, 5 minutes (the default), 15 minutes,
     1 hour or *Never*.
   - The orange mic dot stays lit for the whole session
     ([Wispr](https://docs.wisprflow.ai/articles/3634682593-why-the-orange-dot-or-mic-indicator-stays-on-after-dictating-ios),
     [Willow](https://help.willowvoice.com/en/articles/12855770-why-do-i-see-the-yellow-microphone-indicator-or-floating-island-that-says-on-when-i-use-willow-on-ios)).

### Full Access

The keyboard declares `RequestsOpenAccess`, and you turn it on in Settings.

- **Without Full Access**, a keyboard has:
  - no network,
  - **read-only** access to the shared container,
  - and on iOS 26, no way to open its app.
- **With Full Access** it gets:
  - the network,
  - a writable shared container,
  - and the pasteboard.

una needs Full Access for the hand-off itself: opening the app, and writing "start" and
"stop" into the container. It doesn't need it for networking, because the keyboard never
talks to the server
([Apple](https://developer.apple.com/documentation/uikit/configuring-open-access-for-a-custom-keyboard)).
iOS warns that a full-access keyboard "can transmit anything you type". una's keyboard sends
nothing at all.

### Memory

Apple publishes no number. Reports range from about 30 MB (2018) to 60–70 MB measured, with a
jetsam kill at 77 MB on an iPhone 14 Pro (2026). Above the limit, the system kills the
keyboard and silently swaps in the previous one. So the keyboard holds:

- no audio,
- no model,
- no Go runtime and no mesh,
- and no large frameworks.

It is a small UIKit view and an IPC client.

### Other limits

- **Fields it can't reach.** Secure fields (passwords) and phone-pad fields always get the
  system keyboard. An app can refuse all custom keyboards.
- **Little context.** `documentContextBeforeInput` returns a truncated stretch before the
  cursor, sometimes only the last sentence or two, and it varies by host app.
- **Alternatives to a keyboard.** For later, not the first build:
  - An App Intent or Action Button shortcut that records and leaves the text on the
    clipboard. iOS 18's `AudioRecordingIntent` must start a Live Activity, or recording
    stops.
  - A Control Center control.

### The shape this forces

The keyboard is a remote control; the app is the recorder.

```text
 host app (Messages)          una keyboard                    una app (background)
 ───────────────────          ────────────                    ────────────────────
                              tap mic ──── no session? ──────► opens via una://listen
                                                              starts AVAudioEngine,
                              ◄──── you tap "◀ Messages" ──── keeps it running (orange dot)
                              tap mic ── Darwin notify ──────► marks utterance start
                              level bars ◄── shared file ──── writes levels ~15×/s
                              tap stop ─ Darwin notify ──────► cuts WAV, POSTs to una
                              insertText ◄── App Group ────── writes result, notifies
```

- **App Group** (`group.<your-prefix>.una`) is the shared container: settings, the server
  list, the last result, the level meter, and the spool. **Darwin notifications**
  (`CFNotificationCenterGetDarwinNotifyCenter`) carry the "start", "stop", "result ready"
  pings; they carry no payload, so the payload is always in the container.
- **The app does all networking and all audio.** The keyboard never opens a socket, holds
  no audio and stays far under its memory limit. That is also why the keyboard's choice of
  remote-access option (section 4) doesn't matter: only the app's does.
- **The mic is open for the whole session, not just while you talk.** iOS won't let an app
  *start* recording from the background, so the engine has to keep running between
  utterances; the app throws away every buffer outside a marked utterance and keeps
  nothing on disk. The orange dot says so honestly. A session ends after an idle timeout
  (5 minutes by default, adjustable) or from the app, and then the next tap bounces again.

### What it feels like

- **Once**: install, open una, enter or scan the server, allow the microphone and Local
  Network prompts, add the keyboard in Settings → General → Keyboard → Keyboards, turn on
  **Allow Full Access**.
- **First dictation of a session**: switch to una with the 🌐 key, tap the mic, the screen
  jumps to una for about half a second ("Listening — go back"), tap **◀ Messages** in the
  top-left corner, and you're back with the keyboard showing live level bars. That bounce
  is the price of the microphone rule; every dictation keyboard on iOS has it.
- **Every dictation after that, until the session ends**: tap, talk, tap. The text lands
  about as fast as on the desktop (tenet takes ~1.8 s for a typical dictation, plus the
  upload). Tap-and-hold-to-talk works as well as tap-to-toggle.
- **The keyboard itself** is a **full typing keyboard with dictation built in** (Justin's
  call, 2026-10-22), matching Apple's as closely as a third party can:
  - Letter layout (QWERTY, 10/9/7 rows), Apple's key sizes and spacing for the device
    width, rounded keys with the shadow line, and light and dark appearance following
    `traitCollection`.
  - Press popups on letter keys, haptics on each key (needs Full Access:
    `UIImpactFeedbackGenerator`), and the click sound via `UIDevice.playInputClick()`.
  - Shift with double-tap caps lock and auto-capitalise from
    `documentContextBeforeInput`; delete with press-and-hold repeat (accelerating).
  - The numbers page and the symbols page, return (label follows `returnKeyType`), space
    (long-press for the trackpad cursor, double-space for a period), and the globe key
    (`handleInputModeList`, `needsInputModeSwitchKey`).
  - The **mic is a first-class key** beside the space bar; a suggestion strip above the
    keys holds the last result with *Fix* and an error line.
  - Out of scope for the first cut: autocorrect and the suggestion engine, emoji, and
    languages beyond English layouts. The system offers none of these to third parties for
    free, so what you get is typing without Apple's predictions; say so before it surprises.

  The keyboard stays a remote control for the recording (section above); it adds drawing
  and key handling, not audio or networking.
- **Where it can't go**: password fields and phone-number pads always get Apple's keyboard,
  and an app may refuse third-party keyboards altogether.

## 3. How the app talks to una

The phone is a client of the existing `/v1` API, exactly like `una-core`
(`client/crates/una-core/src/net.rs`). The server needs no changes for the phone to work at
home.

### Endpoints

| Call | Used for |
|---|---|
| `GET /v1/health` | Probing candidate addresses (first 2xx wins), and the status line. |
| `POST /v1/dictations` | One utterance. Multipart: `audio` (file), `utterance_id` (UUID v4), `clean=true`, `client` (`una-ios/<version> ios-arm64`), optional `app_name`, optional `language`. Returns `id`, `text` (what to insert), `raw_text`, `cleaned_text`, `cleanup_applied`, `cleanup_error`, `timings`, `asr_model`. |
| `PUT /v1/dictations/{id}/correction` | A fix or a review verdict (below). |
| `GET /v1/review/queue`, `GET /v1/review/next?after=` | The review tab. |
| `GET /v1/dictations/{id}`, `GET /v1/dictations/{id}/audio` | The fix sheet: both texts and the recording. |
| `GET /v1/dictations` | Recent history in the app. |
| `GET/POST/PATCH /v1/dictionary` | Optional: add a word from the phone. |

Rules carried over from the desktop client, because the server depends on them:

- **One `utterance_id` per utterance, reused on retry.** The server returns the stored
  result for a repeated id, so a retry can never produce a duplicate. Retry exactly once,
  automatically, on a connect or reset error; never on an HTTP status.
- **503 means a training run has the GPU** (`TrainingInProgress`). Say so and keep the
  audio for *Retry*; don't treat it as "server down".
- **Timeouts**: connect 3 s, whole request 60 s, health probe 1.5–2.5 s.
- **Keep failed audio.** Spool the newest five failed utterances in the app's container,
  as `spool.rs` does, with a *Retry* in the app and the keyboard.
- Utterances under 300 ms are dropped; recording stops itself at 5 minutes.

`app_name` feeds the cleanup model's sense of where the text is going. A keyboard extension
cannot reliably learn which app it is typing into (there is no public API for the host's
bundle id), so the phone sends none, or a coarse value you pick per dictation later.

### Audio

Send what the desktop sends: **16 kHz mono 16-bit PCM WAV**. `AVAudioEngine` captures at the
hardware rate (usually 48 kHz float) and an `AVAudioConverter` brings it down to 16 kHz Int16;
the 44-byte WAV header is written by hand. That is 32 kB per second of speech: a 10-second
utterance is about 320 kB, a full minute about 1.9 MB.

The server accepts more than WAV — it decodes whatever libsndfile reads and resamples
(`server/src/una_server/services/storage.py`) — so FLAC (roughly half the size, written by
`ExtAudioFile` on iOS) is a later optimisation for slow cellular uplinks, not a requirement.
AAC and CAF-wrapped Opus, iOS's natural compressed formats, are **not** readable by
libsndfile; using them would need a server-side decoder.

### Corrections on a phone

The desktop learns from what you do to text after it lands. A phone keyboard can't watch
that: once the text is in the field, edits made with Apple's keyboard are invisible to una,
and the una keyboard only ever sees a few hundred characters around the cursor. So the
phone does not file automatic `accepted` verdicts — an untouched-looking dictation may well
have been fixed by hand — and every phone dictation stays in the review queue until it is
judged. The meaning of each verdict is the one in [learning.md](learning.md) and
[personal-model.md](personal-model.md); the phone must not blur literal and polished.

Three ways to judge a dictation, cheapest first:

1. **Fix chip in the keyboard.** After inserting, the keyboard shows the last dictation's
   text in a small strip with *Fix*. Tapping it opens the app's fix sheet. If the cursor is
   still right after what was inserted (checked with `documentContextBeforeInput`), the
   keyboard can swap the fixed text in place: `deleteBackward()` over the inserted
   characters, then `insertText(_:)`.
2. **Fix sheet in the app** — the desktop fix window on a phone: the recording with a play
   button, **What you said** (the raw transcript) and **How it should read** (what was
   inserted). Saving sends `PUT …/correction` with `transcript_shown: true`, the `edited` or
   `accepted` action, `corrected_text` and `polished_text`, so the server reads it as a
   verdict on what was said, as it does for the desktop fix window.
3. **Review tab** — the desktop review window: the queue one at a time, both texts
   pre-filled, Confirm / Skip / Exclude, sent with `source: "review"`.

The dashboard's Review page already works in Safari from any address that reaches the
server, so review is possible on the phone before any of this is built.

### Finding the server

The app keeps an ordered list of addresses and races them, like `endpoint.rs`: every
candidate is probed at once with `GET /v1/health`, the first answer wins and is cached for
five minutes, and the cache is dropped when an upload fails. On iOS there is a better
trigger than failure: `NWPathMonitor` reports every Wi-Fi ↔ cellular change, so the app
re-probes before the next dictation rather than after a failed one.

A starting list:

```text
http://192.168.1.172:8100        tenet on the home LAN
http://tenet.makima:8100         tenet over makima; only once the phone is on makima (option 1)
<remote address>                 whatever section 4 decides
```

- **Bonjour.** The server advertises `_una._tcp` (`services/discovery.py`). The app can
  browse it with `NWBrowser` and needs no multicast entitlement for it. It must list
  `_una._tcp` under `NSBonjourServices` and give an `NSLocalNetworkUsageDescription`, both
  in the *app's* Info.plist. The keyboard shares the app's permission.
- **The Local Network prompt.** Any LAN address, browsed or typed, triggers iOS's *Local
  Network* prompt the first time. It must be asked during onboarding, in the foreground: an
  app that hits an undecided permission from the background is silently denied. Cellular and
  VPN interfaces are not "local", so a remote path never prompts
  ([TN3179](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy)).
- **Plain HTTP and ATS.** App Transport Security blocks `http://` by default.
  - **`NSAllowsLocalNetworking`** allows IP addresses, unqualified names and `.local` names.
    That covers the LAN entries, loopback and Tailscale's `100.x` addresses. iOS 17 stopped
    allowing bare IPs without it.
  - **Dotted names** like `tenet.makima` or `*.ts.net` are not covered. They need an
    `NSExceptionDomains` entry with `NSExceptionAllowsInsecureHTTPLoads`, or real HTTPS.
  - **`NSAllowsArbitraryLoads`** would do too. It's fine for a sideloaded build but needs a
    justification in App Review.
  - The desktop hit the same wall with its WKWebView.
- **Pin tenet's LAN address.** There's no name to fall back on at home: `tenet.local`
  doesn't resolve, because avahi is off on tenet. A DHCP reservation for 192.168.1.172 on
  the router keeps the LAN entry true.
- **Setup by QR code (later).** The dashboard could show a QR code holding the address list
  (and a token, if section 4 needs one); the app scans it instead of you typing URLs.

## 4. Reaching tenet away from home

**Decided: option 2, Tailscale** (Justin, 2026-10-22; see the end-to-end checklist under
option 2). The other options stay below as the record of what was weighed. At home the phone reaches
`http://192.168.1.172:8100` directly. Away from home, something has to carry it there. The
options fall into two families:

- **A private network** (makima, Tailscale, WireGuard): the phone joins a network tenet is
  already on. una stays as it is, unauthenticated plain HTTP, because nobody outside that
  network can reach it. The cost is a VPN on the phone. iOS runs **one VPN at a time**, so
  this takes the slot a work VPN or another mesh would want.
- **A public endpoint** (Cloudflare Tunnel, port forwarding, Funnel): una becomes reachable
  from the internet behind TLS. No VPN is involved, but una **must gain authentication**
  first: today anyone who reaches port 8100 can read every recording and swap the model
  ([remote-access.md](remote-access.md)).

Whichever you pick, only the **app's** traffic matters. The keyboard never touches the
network, so its memory limit doesn't constrain the choice.

Some numbers that hold for every option. Upload size dominates, not round trips. A
10-second utterance is ~320 kB, which is ~0.5 s on a 5 Mbps cellular uplink and ~2.6 s on
1 Mbps. Next to that, a relay hop of 20–80 ms barely shows. Tor is the exception (below).
The biggest battery cost is the open microphone during a session, which is the same for
every option. A VPN adds a little on top; an always-on one adds a little more.

### Option 1: makima on the phone

**What exists today: nothing for phones.** makima has no iOS or Android client, no
gomobile build and no Network Extension code. Its builds are darwin, linux and windows only.
`makima migrate` deliberately leaves phones on Tailscale ("makima does not run on iPhone or
iPad yet", `internal/migrate/plan.go` in the makima repo). It could gain one of two shapes.

**1a. A makima VPN app (a Network Extension packet tunnel, like Tailscale's).**
- *What you'd get:* every app on the phone sees `*.makima`, Safari's dashboard included.
- *What it takes:*
  - wireguard-go builds for iOS: the official WireGuard app uses it.
  - A Swift tunnel provider.
  - The managed join, the netmap long poll and the network-lock check. The lock check
    lives in makimad's `package main` today, so it would have to be moved out.
  - DNS for `*.makima` set through the tunnel's settings.
- *The blocker: tor.* This mesh's control server runs on midna and is reached over its
  onion address, and the relay is onion-only. makima starts tor as a child process, which
  iOS forbids. So either an embedded tor goes inside the extension, under its memory limit,
  or makima first gets a clearnet control address and relay, for example on a VPS or on a
  home node with a forwarded port.
- *Signing:* needs the Network Extension entitlement, so **paid account only**.
- *Size:* weeks of work in the makima project, before the una app needs anything.

**1b. makima inside the una app, in userspace (no VPN).**
- *The seed exists.* makima's rootless mode, the engine behind `makima try`, runs
  wireguard-go on a gVisor netstack inside the process. It already offers `Dial` and
  `Forward`.
- *How the app would use it:*
  - Link a Go framework built from it.
  - Forward a local port, say `127.0.0.1:18100`, to `tenet.makima:8100`.
  - Point URLSession at the local port, which `NSAllowsLocalNetworking` covers.
- *What's missing in makima:*
  - Rootless mode only knows pairing, and managed nodes like tenet refuse to pair, so it
    can't reach tenet today. It would need the managed join, the netmap long poll and the
    lock check.
  - All of it is under makima's `internal/`, so it needs a wrapper package inside the
    makima module.
  - The same tor problem as 1a.
- *Signing:* uses no entitlement, so it **works with free signing**. It doesn't take the VPN
  slot.
- *Scope and timing:* it carries only una's own traffic, and only while the app runs. The
  app is running anyway whenever a session is live. The mesh comes up when a session starts
  and overlaps the bounce to the app. That holds only if control is reachable directly:
  tor's cold start can take most of a minute.
- *Memory:* the Go runtime plus the netstack in the app is not measured; tens of MB is a
  guess. That's fine for the app and impossible in the keyboard.

**For both shapes:**
- *Security:* WireGuard end to end, and keys stay on your own machines. But a phone joining
  as an ordinary node is a big grant. makima's default policy is allow-all, and every
  service a peer has on 127.0.0.1 below port 49152 is auto-published to the mesh: Ollama,
  dev servers, makima's sshd. The phone should join with a tagged auth key whose policy
  allows only `tenet.makima:8100`, and be revocable at the control server if it's lost. With
  the network lock on, its node key also has to be signed when it enrolls.
- *Latency:* direct WireGuard when hole punching succeeds is as good as any option. When it
  fails (cellular NATs often defeat it), traffic falls back to the relay, which today means
  **tor**: seconds per upload, too slow for dictation. A clearnet relay fixes that.
- *Battery:* 1a is always on, like any mesh VPN, plus whatever tor costs. 1b runs only during
  sessions.
- *What una adds:* nothing on the server. 1a needs an ATS exception for `.makima`. 1b talks
  to loopback, which `NSAllowsLocalNetworking` covers.
- *Cost:* $0 for 1b; 1a needs the $99 account. A clearnet relay on a VPS is ~$4–6 a month,
  which both shapes likely need.

### Option 2: Tailscale — CHOSEN

**End to end, what it takes:**

1. **tenet joins the tailnet.** Needs sudo, so Justin or the lead runs it on tenet (Arch
   commands, as in [remote-access.md](remote-access.md#tailscale-setup)):

   ```sh
   sudo pacman -S tailscale && sudo systemctl enable --now tailscaled
   sudo tailscale up
   ```

   `tailscale up` prints a login URL; sign in as the account the phone will use. Then, in
   the admin console, **disable key expiry** for tenet. Read off its names with
   `tailscale ip -4` (the `100.x.y.z`) and `tailscale status --json | grep -i dnsname`
   (the `tenet.<tailnet>.ts.net` MagicDNS name). If tenet already runs Tailscale, only the
   last step is needed.
2. **The phone** installs Tailscale from the App Store and signs into the same tailnet.
   Free signing is fine: Tailscale's own app carries the VPN entitlement, not una's.
3. **una listens on all interfaces.** `bind` defaults to `0.0.0.0`
   (`server/src/una_server/config.py`), so traffic arriving on `tailscale0` reaches port
   8100 with no change. If tenet's firewall is on, `tailscale0` must be allowed to reach
   8100 (`sudo firewall-cmd --permanent --zone=trusted --add-interface=tailscale0`
   on firewalld; with ufw, `sudo ufw allow in on tailscale0 to any port 8100`). Check from
   the phone with Tailscale on and Wi-Fi off.
4. **The app's server list** (entered by the user in onboarding; the app preconfigures
   nothing) holds three addresses, raced as today: the LAN address, `http://tenet.<tailnet>.ts.net:8100` (MagicDNS), and `http://100.x.y.z:8100`
   (the tailnet IP, which works if MagicDNS is off). The first answer wins, so at home the
   LAN address wins and away the tailnet ones do. No client change: they are ordinary
   entries. The second name needs the tailnet name filled in once tenet has joined.
5. **ATS and plain http.** `Config/Una-Info.plist` sets `NSAllowsArbitraryLoads` (see the
   checklist in section 5), which covers the MagicDNS name and any other hostname the user
   enters; the `ts.net` and `tenet.makima` exceptions it replaces are gone. Plain http is acceptable here because
   WireGuard already encrypts the hop. Reaching a LAN or `100.x` address from the app also
   triggers the **Local Network** prompt once.
6. **Optional hardening:** an ACL that limits the phone to `tenet:8100`, and VPN On Demand
   so the VPN is up only off home Wi-Fi.

**Details of the option:**

- *How:* the Tailscale app on the phone (App Store) and `tailscaled` on tenet, in one
  tailnet. The app's list gains `http://tenet.<tailnet>.ts.net:8100` and tenet's `100.x`
  address. [remote-access.md](remote-access.md) already covers the server side, including
  disabling tenet's key expiry.
- *What's running now:* on this Mac, Tailscale.app is installed but stopped. Whether tenet
  still runs it is to be confirmed. makima uses its own address range so the two can run
  side by side, so tenet can run both.
- *Security:*
  - WireGuard end to end, and no open ports.
  - Tailscale Inc.'s coordination server sees which devices exist and when they connect.
    Its DERP relays carry only encrypted packets.
  - An ACL can limit the phone to `tenet:8100`.
  - A lost phone is removed in the admin console.
- *Cost:* the free Personal plan: up to 6 users and unlimited devices, as of
  [April 2026](https://tailscale.com/pricing).
- *Setup:* about 15 minutes. You keep two meshes, makima for the machines and Tailscale for
  the phone.
- *Signing:* none needed. Tailscale's app carries the Network Extension entitlement, so it
  **works with free signing**. It takes the VPN slot.
- *What una adds:*
  - Nothing on the server.
  - In the app, an ATS exception for `ts.net`, or use the `100.x` IP, which
    `NSAllowsLocalNetworking` covers.
- *Latency:* direct WireGuard most of the time. A DERP relay, when used, adds tens of ms.
- *Battery:* an always-on VPN with a control connection costs a little.
  [VPN On Demand](https://tailscale.com/kb/1291/ios-vpn-on-demand) can limit it to when
  you're off home Wi-Fi.
- *Variant, Tailscale inside the app:* `tsnet` (Tailscale as a Go library) runs inside an
  app's own process with no entitlements and carries only that app's connections, through a
  loopback proxy ([an example](https://github.com/indiagrams/tunnelless)). That is option
  1b's shape with Tailscale's code: free signing, no VPN slot, live only during sessions,
  and a few days of work in una's app. App Store acceptance of the approach is unverified.
- *Variant, Headscale:* a self-hosted Tailscale control server. The iOS app can point at a
  custom server, so Tailscale Inc. drops out. It needs a publicly reachable host to run on,
  such as a VPS or a forwarded port.

### Option 3: plain WireGuard (the official app)

- *How:*
  - A `wg` interface on tenet, or on the router if it speaks WireGuard.
  - One UDP port forwarded on the home router.
  - A dynamic-DNS name for the home IP.
  - The phone's config imported from a QR code.
  - On-demand rules that connect everywhere except your home Wi-Fi.
- *Security:*
  - WireGuard doesn't answer packets without a valid key, so a port scan sees nothing.
  - One UDP port is the only exposure.
  - No third party is involved, and the keys are yours.
  - `AllowedIPs` plus a firewall rule can limit the phone to `tenet:8100`.
- *Cost:* $0 with a free dynamic-DNS service.
- *Setup:* about an hour, including sudo on tenet, the router and the DDNS updater.
- *The catch:* it **doesn't work if your ISP puts you behind CGNAT**. Check whether the
  router's WAN IP matches your public IP. If not, the endpoint has to live on a VPS
  (option 6).
- *Signing:* none needed. The WireGuard app carries the entitlement, so it **works with free
  signing**. It takes the VPN slot.
- *What una adds:* nothing. The address is an IP, which `NSAllowsLocalNetworking` covers.
- *Latency:* the lowest of the VPN options. The path is always direct, with no hole punching
  to fail.
- *Battery:* the best of the VPN options. WireGuard is silent when idle and has no control
  plane.
- *Separate from makima:* this is its own tunnel. Whether makima's managed mesh could take a
  hand-added plain-WireGuard peer is unverified: its `makima peer` command is for meshes
  maintained by hand.

### Option 4: Cloudflare Tunnel, with auth added to una

- *How:* `cloudflared` on tenet dials out to Cloudflare, and `https://una.<your-domain>`
  forwards to `http://127.0.0.1:8100`.
  - No open ports.
  - Works behind CGNAT.
  - Works on any network that allows HTTPS, hotel Wi-Fi included.
- *Security:*
  - The endpoint is public, so lock it twice:
    - Cloudflare Access in front, with a service token for the app (two request headers)
      and a browser login for the dashboard.
    - una's own token behind it.
  - **Cloudflare terminates TLS, so it sees your audio and transcripts in clear.** It is the
    only option where a third party sees content, not just metadata.
- *Cost:* a domain on Cloudflare DNS (~$10 a year). Access is free at this size.
- *Setup:* about an hour, plus the una work.
- *Signing:* free signing is fine, and it **uses no VPN slot**.
- *What una adds:*
  - **Authentication:** per-device bearer tokens, stored hashed, created and revoked from
    the dashboard, paired by QR code.
  - Token support in the desktop client's `net.rs`, if the desktop uses this address too.
  - A login for the dashboard, or don't expose it.
  - A decision on which routes are reachable from outside. Settings, model activation and
    training probably shouldn't be.
- *Latency:* one extra hop through the nearest Cloudflare edge, roughly 10–40 ms.
  Cloudflare's upload limit (100 MB on the free plan) is far above a dictation.
- *Battery:* the best. Nothing runs when you're not dictating.
- *Variant, Tailscale Funnel:* a public HTTPS name under `ts.net` with no domain to buy. It's
  in beta, limited to ports 443, 8443 and 10000, and has a bandwidth cap
  ([docs](https://tailscale.com/kb/1223/funnel)). It's just as public, so it needs the same
  una auth.

### Option 5: port forwarding with TLS and auth

- *How:* the router forwards TCP 443 to tenet. Caddy there holds a Let's Encrypt
  certificate for a dynamic-DNS name and proxies to una, which is bound to `127.0.0.1`.
- *Security:* the weakest as plain token auth. Your home IP and a TLS endpoint are public,
  and everything then rests on una's auth and Caddy.
- *The strongest variant, mutual TLS:*
  - The phone gets a client certificate, kept in the app's keychain.
  - Caddy drops any connection without one, so scanners never reach una at all.
  - URLSession supports client certificates through its auth challenge, with no
    entitlement needed.
- *Cost:* $0.
- *Setup:* about 2 hours: router, DDNS, Caddy, certificates, firewall. CGNAT rules it out,
  and some ISPs block inbound 443.
- *Signing:* free signing is fine. No VPN slot.
- *What una adds:* the same auth as option 4. The mTLS variant also needs certificate
  issuing and revoking.
- *Latency:* direct, so the lowest.
- *Battery:* the best.

### Option 6: relay through another machine

- *Why:* useful when home can't be reached from outside, because of CGNAT or because you'd
  rather not open the home router.
- *How:* a machine with a public IP, usually a small VPS, takes the phone's connection and
  forwards it over makima to `tenet.makima:8100`. The VPS joins makima as a node. Toward the
  phone it runs either:
  - WireGuard: option 3 without touching the home router, or
  - Caddy with auth: options 4 and 5 without a third party seeing content.
- *What it does for option 1:* the same box can become makima's clearnet control address and
  relay, which is exactly what option 1 is missing.
- *Why not midna:* midna is at home behind the same router as tenet, so it adds no
  reachability unless a port is forwarded to it.
- *Security:* the VPS is one more machine to harden. Traffic is decrypted on it between the
  two tunnels, but it's your machine.
- *Cost:* ~$4–6 a month.
- *Setup:* half a day.
- *Latency:* one more hop, 10–40 ms, depending on where the VPS is.
- *Battery and signing:* the same as whatever runs between the phone and the VPS.

### Others

- **ZeroTier.** Its own App Store VPN app, a mesh like Tailscale's, with the same trade-offs.
  It's another network to keep beside makima.
- **An SSH tunnel from the app.** The app embeds an SSH client and forwards a port through a
  public SSH host. It works, but it's clumsier than WireGuard for the same exposure.
- **No remote path, queue instead.** Away from home, the app falls back to Apple's on-device
  speech recognition for text right now and holds the audio. It uploads the audio to una
  when you're home, for history and training. Nothing is exposed. The text away from home
  isn't your model's, though. This combines with any option above as the offline fallback.

### Side by side

| Option | Security | Cost | Setup | Signing | una adds | Latency | Battery | Build (step 4) |
|---|---|---|---|---|---|---|---|---|
| 1a makima VPN app | strong, your keys; needs scoped ACL | $99/yr (+ VPS relay) | high (makima work) | **paid** | nothing | direct; tor relay today | always-on VPN | weeks, mostly in makima |
| 1b makima in app | strong, your keys; needs scoped ACL | $0 (+ VPS relay) | high (makima work) | free | nothing | direct; tor relay today | sessions only | 2–3 weeks, mostly in makima |
| 2 Tailscale | strong; Tailscale sees metadata | $0 | 15 min | free | nothing | direct, DERP fallback | VPN, on-demand | ½ day |
| 2 variant: tsnet in app | strong; Tailscale sees metadata | $0 | 15 min + build | free | nothing | as 2 | sessions only | a few days |
| 3 WireGuard | strong; one silent UDP port | $0 | ~1 h; no CGNAT | free | nothing | direct, lowest | VPN, on-demand, low | ½ day |
| 4 Cloudflare Tunnel | public; CF sees content | ~$10/yr | ~1 h | free | **auth** | +edge hop | none | 3–5 days |
| 5 Port forward + TLS | public; mTLS makes it strong | $0 | ~2 h; no CGNAT | free | **auth** (or mTLS) | direct, lowest | none | 3–6 days |
| 6 Relay via VPS | as its front end | $4–6/mo | ½ day | free | per front end | +1 hop | per front end | ½ day + front end |

## 5. Build plan

Layout under `client/apps/ios/`:

```text
client/apps/ios/
  Una.xcodeproj        with folder-synchronized groups (Xcode 16+), so adding a file
                       doesn't touch project.pbxproj and agents don't fight over it
  Una/                 the app (SwiftUI): onboarding, record, history, fix sheet, review, settings
  UnaKeyboard/         the keyboard extension (UIKit view controller hosting SwiftUI)
  UnaKit/              Swift package shared by both: API client, endpoint race, WAV
                       encoder, spool, App Group IPC, models matching schemas.py
```

Swift only, no third-party packages, **iOS 17 minimum** (Justin, 2026-10-22: iOS 17 and 18
run on the same iPhones, XS/XR and newer, so 17 costs no devices). Nothing that needs iOS 18
may be used without an `if #available` fallback (the new Tab API, mesh gradients and the
like).

**General-purpose rule.** Anyone should be able to download the app, so nothing in it names
Justin: no server addresses, hostnames, personal ids, paths or sample data. The server
address, the away-from-home setup and (later) pairing all go through first-run
onboarding (`Una/OnboardingView.swift`), and the server list starts empty. The bundle id and
App Group id live in one place, `Config/Una.xcconfig` (`UNA_BUNDLE_ID`, default
`org.example.una`; the group is `group.$(UNA_BUNDLE_ID)`), and are changed there when
signing. Test fixtures use neutral placeholders such as `http://192.168.1.10:8100`.

**App Store-ready checklist**, even while it sideloads:

- [x] Privacy usage strings: microphone and local network (in the build settings).
- [x] `Una/PrivacyInfo.xcprivacy`: no tracking, no collected data types (audio goes only to
  the user's own server), and the required-reason APIs declared (UserDefaults `CA92.1`,
  file timestamps `C617.1`).
- [x] A Full Access explanation in the app (onboarding and the Servers tab, one shared text).
- [ ] The same explanation in the keyboard itself (step 2).
- [x] No analytics, no third-party packages. No private APIs (keep it so).
- [ ] ATS: the app sets `NSAllowsArbitraryLoads` because the user's own server is plain
  http at an address nobody can predict. App Review asks for a justification for that key;
  the answer is "the app connects only to a server address the user types". Revisit if
  review pushes back (the fallback is `NSAllowsLocalNetworking` plus per-domain exceptions).
- [ ] Real bundle id, App Group, signing and an app icon, once the paid account exists. Unit
tests for UnaKit (WAV header, endpoint race, response decoding against fixtures captured
from the real server); the mic, background audio and the keyboard only behave truthfully on
a device, so those are checked by hand on your iPhone.

Rough sizes, in focused working days, for one developer:

| Step | What | Days |
|---|---|---|
| 0 | **Spike on your phone, under free signing**: app + keyboard install side by side, App Group shared between them, keyboard opens the app, recording carries on after you go back, Darwin notifications both ways. Everything else rests on this. | 1–2 |
| 1 | **App MVP**: server list with *Test all*, record → 16 kHz WAV → upload, result copied to the clipboard, history, spool and retry. Usable at home on its own. | 3–4 |
| 2a | **Typing keyboard**: letters, shift and caps lock, delete with repeat, numbers and symbols pages, return, space, globe, popups, haptics, light and dark, device-size metrics. Measure memory in the simulator against the extension limit and record the headroom. | 6–9 |
| 2b | **Mic as remote control**: mic key, session hand-off to the app (`una://listen`), App Group and Darwin notifications, live level meter, insert, errors (no server, Full Access off, training running), session timeout. | 4–6 |
| 3 | **Corrections**: fix chip and in-place swap, fix sheet with playback, review tab. | 3–4 |
| 4 | **Away from home: Tailscale** (chosen): tenet joins the tailnet, the phone installs the app, two addresses added to the list (section 4, option 2). | ½ |
| 5 | **Polish**: onboarding, haptics, path-change re-probe, this doc rewritten as a how-to. | 2–3 |

About four to five weeks to a keyboard you'd use daily (the full typing keyboard roughly
doubles step 2), plus half a day for Tailscale.
With the paid account, step 0's re-signing chores go away and TestFlight replaces the
weekly cable reinstall; nothing in steps 1–3 changes.

Before step 0 the Mac needs **Xcode** (only the Command Line Tools are installed today),
signed into your Apple ID in Xcode → Settings → Accounts, and the phone needs **Developer
Mode** on (Settings → Privacy & Security, after the first install attempt).

## What's needed from you

- **Section 4 is decided: Tailscale.** Still needed: the sudo commands on tenet (option 2,
  step 1), and the tailnet name once tenet has joined.
- **The phone.** Which iPhone, and which iOS version.
- **Xcode** on this Mac, signed into the Apple ID you'll use (it isn't installed today).
- **Home network**, for options 3 and 5: whether the ISP uses CGNAT (compare the router's
  WAN IP with your public IP), and a DHCP reservation for tenet.
- **A domain**, for option 4: whether you have one on Cloudflare.
